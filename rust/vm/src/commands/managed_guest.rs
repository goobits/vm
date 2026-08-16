//! Controller-owned settings reconciled into managed environments.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vm_packages::ManagedClientSettings;
use vm_provider::Provider;

use crate::error::{VmError, VmResult};

pub(crate) const GUEST_REMOTE_COMMANDS_PATH: &str = "/etc/vm/remote-commands.json";
pub(crate) const REMOTE_COMMAND_SCHEMA: u8 = 1;
const CONTROLLER_REGISTRY: &str = ".vm/remote-commands.json";
const MAX_REGISTRY_BYTES: u64 = 1024 * 1024;

pub(crate) const INSTALL_MANAGED_SETTINGS: &str = r#"import json, os, pathlib, sys, tempfile

request = json.load(sys.stdin)
if request.get("schema") != 1 or not set(request).issubset({"schema", "package", "remote_commands", "remove_remote_commands"}):
    raise SystemExit("invalid VM managed guest settings")

uid = int(os.environ.get("SUDO_UID", "0"))
gid = int(os.environ.get("SUDO_GID", "0"))
sensitive_mode = 0o640 if uid else 0o600

def managed_directory(path, mode=0o755, owner=None):
    path = pathlib.Path(path)
    if path.is_symlink():
        raise SystemExit(f"refusing managed directory symlink: {path}")
    path.mkdir(parents=True, exist_ok=True)
    metadata = path.stat()
    if (metadata.st_mode & 0o777) != mode:
        os.chmod(path, mode)
    if owner and (metadata.st_uid, metadata.st_gid) != owner:
        os.chown(path, *owner)
    return path

def replace(path, content, mode=0o644, owner=None):
    path = pathlib.Path(path)
    if path.is_symlink():
        raise SystemExit(f"refusing managed file symlink: {path}")
    encoded = content.encode()
    if path.is_file() and path.read_bytes() == encoded:
        metadata = path.stat()
        if (metadata.st_mode & 0o777) == mode and (not owner or (metadata.st_uid, metadata.st_gid) == owner):
            return
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as output:
            output.write(encoded)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary, mode)
        if owner:
            os.chown(temporary, *owner)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)

managed_directory("/etc/vm", 0o750 if uid else 0o700, (0, gid))
owner = (0, gid)

package = request.get("package")
if package is not None:
    required = {"revision", "profile", "npmrc", "pip_conf", "cargo_config"}
    if set(package) != required or not all(isinstance(package[key], str) for key in required):
        raise SystemExit("invalid VM package client settings")
    managed_directory("/etc/profile.d")
    replace("/etc/profile.d/vm-packages.sh", package["profile"], sensitive_mode, owner)
    replace("/etc/vm/npmrc", package["npmrc"], sensitive_mode, owner)
    replace("/etc/vm/pip.conf", package["pip_conf"], sensitive_mode, owner)
    replace("/etc/vm/cargo-config.toml", package["cargo_config"], sensitive_mode, owner)
    replace("/etc/vm/package-client.revision", package["revision"] + "\n", sensitive_mode, owner)
    replace("/etc/vm/managed-guest", "1\n", 0o644, (0, 0))

    source = "[ -r /etc/profile.d/vm-packages.sh ] && . /etc/profile.d/vm-packages.sh"
    for candidate in ("/etc/bash.bashrc", "/etc/zsh/zshrc"):
        path = pathlib.Path(candidate)
        if not path.is_file():
            continue
        if path.is_symlink():
            raise SystemExit(f"refusing shell configuration symlink: {path}")
        content = path.read_text()
        if source not in content.splitlines():
            replace(path, content.rstrip("\n") + "\n" + source + "\n")

remote = request.get("remote_commands")
remote_path = pathlib.Path("/etc/vm/remote-commands.json")
if remote is not None:
    if remote.get("schema") != 1 or not isinstance(remote.get("commands"), dict):
        raise SystemExit("invalid VM remote command settings")
    replace(remote_path, json.dumps(remote, sort_keys=True, separators=(",", ":")) + "\n", sensitive_mode, owner)
elif request.get("remove_remote_commands", False):
    if remote_path.is_symlink():
        raise SystemExit(f"refusing managed file symlink: {remote_path}")
    remote_path.unlink(missing_ok=True)
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GuestRemoteCommands {
    pub(crate) schema: u8,
    pub(crate) commands: BTreeMap<String, RemoteCommandRegistration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteCommandRegistration {
    pub(crate) endpoint: String,
    pub(crate) capability: String,
    pub(crate) repair_command: String,
}

#[derive(Deserialize)]
struct ControllerRegistry {
    schema: u8,
    environments: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize)]
struct InstallRequest<'a> {
    schema: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<&'a ManagedClientSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_commands: Option<&'a GuestRemoteCommands>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    remove_remote_commands: bool,
}

enum RemoteSettings {
    Unconfigured,
    Remove,
    Install(GuestRemoteCommands),
}

pub(crate) fn install_package_settings(
    provider: &dyn Provider,
    environment: &str,
    settings: &ManagedClientSettings,
) -> VmResult<()> {
    install(
        provider,
        environment,
        &InstallRequest {
            schema: REMOTE_COMMAND_SCHEMA,
            package: Some(settings),
            remote_commands: None,
            remove_remote_commands: false,
        },
    )
}

pub(crate) fn reconcile_remote_commands(
    provider: &dyn Provider,
    environment: &str,
) -> VmResult<()> {
    match remote_settings(&controller_registry_path(), environment)? {
        RemoteSettings::Unconfigured => Ok(()),
        RemoteSettings::Remove => install(
            provider,
            environment,
            &InstallRequest {
                schema: REMOTE_COMMAND_SCHEMA,
                package: None,
                remote_commands: None,
                remove_remote_commands: true,
            },
        ),
        RemoteSettings::Install(settings) => {
            crate::commands::remote_command::validate_registry(&settings)?;
            install(
                provider,
                environment,
                &InstallRequest {
                    schema: REMOTE_COMMAND_SCHEMA,
                    package: None,
                    remote_commands: Some(&settings),
                    remove_remote_commands: false,
                },
            )
        }
    }
}

fn install(
    provider: &dyn Provider,
    environment: &str,
    request: &InstallRequest<'_>,
) -> VmResult<()> {
    let content = serde_json::to_vec(request)
        .map_err(|error| VmError::general(error, "Failed to render managed guest settings"))?;
    let command = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "if [ \"$(id -u)\" -eq 0 ]; then exec python3 -c \"$1\"; else exec sudo -n python3 -c \"$1\"; fi".to_string(),
        "vm-managed-settings".to_string(),
        INSTALL_MANAGED_SETTINGS.to_string(),
    ];
    provider
        .exec_with_stdin(Some(environment), &command, &content)
        .map_err(VmError::from)
}

fn controller_registry_path() -> PathBuf {
    if std::env::var_os("VM_TEST_MODE").is_some() {
        if let Some(path) = std::env::var_os("VM_REMOTE_COMMANDS_CONTROLLER_FILE") {
            return path.into();
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONTROLLER_REGISTRY)
}

fn remote_settings(path: &Path, environment: &str) -> VmResult<RemoteSettings> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RemoteSettings::Unconfigured)
        }
        Err(error) => {
            return Err(VmError::filesystem(
                error,
                path.display().to_string(),
                "read",
            ))
        }
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_REGISTRY_BYTES {
        return Err(VmError::validation(
            "Controller remote command registry must be a regular file no larger than 1 MiB",
            Some("Run: vm doctor"),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(VmError::validation(
                "Controller remote command registry is group/world writable",
                Some("Run: chmod 600 ~/.vm/remote-commands.json"),
            ));
        }
    }
    let registry: ControllerRegistry =
        serde_json::from_slice(&fs::read(path)?).map_err(|error| {
            VmError::validation(
                format!("Controller remote command registry is invalid: {error}"),
                Some("Run: vm doctor"),
            )
        })?;
    if registry.schema != REMOTE_COMMAND_SCHEMA {
        return Err(VmError::validation(
            format!(
                "Unsupported remote command registry schema {}",
                registry.schema
            ),
            Some("Run: vm doctor"),
        ));
    }
    let Some(settings) = registry.environments.get(environment) else {
        return Ok(RemoteSettings::Remove);
    };
    let settings = serde_json::from_value(settings.clone()).map_err(|error| {
        VmError::validation(
            format!("Remote commands for '{environment}' are invalid: {error}"),
            Some("Run: vm doctor"),
        )
    })?;
    Ok(RemoteSettings::Install(settings))
}

#[cfg(test)]
mod tests {
    use super::{remote_settings, RemoteSettings, INSTALL_MANAGED_SETTINGS};
    use serde_json::json;
    use std::fs;

    #[test]
    fn selects_only_the_requested_environment() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("remote-commands.json");
        fs::write(
            &path,
            json!({
                "schema": 1,
                "environments": {
                    "demo-dev": {"schema": 1, "commands": {}},
                    "other-dev": "broken but isolated"
                }
            })
            .to_string(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        assert!(matches!(
            remote_settings(&path, "demo-dev").unwrap(),
            RemoteSettings::Install(_)
        ));
        assert!(matches!(
            remote_settings(&path, "missing-dev").unwrap(),
            RemoteSettings::Remove
        ));
    }

    #[test]
    fn one_atomic_installer_owns_package_and_remote_settings() {
        assert!(INSTALL_MANAGED_SETTINGS.contains("os.replace(temporary, path)"));
        assert!(INSTALL_MANAGED_SETTINGS.contains("/etc/profile.d/vm-packages.sh"));
        assert!(INSTALL_MANAGED_SETTINGS.contains("/etc/vm/remote-commands.json"));
        assert!(INSTALL_MANAGED_SETTINGS.contains("refusing managed file symlink"));
    }
}
