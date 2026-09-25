//! Controller-owned settings reconciled into managed environments.

use serde::Serialize;
use vm_packages::ManagedClientSettings;
use vm_provider::CommandProvider;

use crate::error::{VmError, VmResult};

pub(crate) const INSTALL_MANAGED_SETTINGS: &str = r#"import json, os, pathlib, sys, tempfile

request = json.load(sys.stdin)
if request.get("schema") != 1 or not set(request).issubset({"schema", "package"}):
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
replace("/etc/vm/managed-guest", "1\n", 0o644, (0, 0))

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

"#;

#[derive(Serialize)]
struct InstallRequest<'a> {
    schema: u8,
    package: Option<&'a ManagedClientSettings>,
}

pub(crate) fn install_package_settings(
    provider: &dyn CommandProvider,
    environment: &str,
    settings: &ManagedClientSettings,
) -> VmResult<()> {
    install(
        provider,
        environment,
        &InstallRequest {
            schema: 1,
            package: Some(settings),
        },
    )
}

fn install(
    provider: &dyn CommandProvider,
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

#[cfg(test)]
mod tests {
    use super::INSTALL_MANAGED_SETTINGS;

    #[test]
    fn atomic_installer_preserves_package_settings_and_guest_marker() {
        assert!(INSTALL_MANAGED_SETTINGS.contains("os.replace(temporary, path)"));
        assert!(INSTALL_MANAGED_SETTINGS.contains("/etc/profile.d/vm-packages.sh"));
        assert!(INSTALL_MANAGED_SETTINGS.contains("refusing managed file symlink"));
        assert!(INSTALL_MANAGED_SETTINGS.contains("/etc/vm/managed-guest"));
    }
}
