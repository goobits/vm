use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use vm_config::config::{PackageEdgeConfig, VmConfig};
use vm_packages::{ApplianceState, ClientEnvironment, InfrastructureRuntime, RegistryEndpoints};
use vm_provider::Provider;

use crate::commands::command_context::RuntimeSubject;
use crate::error::{VmError, VmResult};

use super::{appliance, files::ApplianceFiles};

// Bump when edge labels, environment, mounts, or lifecycle policy change
// without requiring a new registry image.
const PACKAGE_EDGE_POLICY_REVISION: &str = "2";

const INSTALL_CLIENT_SETTINGS: &str = r#"import json, os, pathlib, sys, tempfile

settings = json.load(sys.stdin)
required = {"revision", "profile", "npmrc", "pip_conf", "cargo_config"}
if set(settings) != required or not all(isinstance(settings[key], str) for key in required):
    raise SystemExit("invalid VM package client settings")

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
managed_directory("/etc/profile.d")
owner = (0, gid)
replace("/etc/profile.d/vm-packages.sh", settings["profile"], sensitive_mode, owner)
replace("/etc/vm/npmrc", settings["npmrc"], sensitive_mode, owner)
replace("/etc/vm/pip.conf", settings["pip_conf"], sensitive_mode, owner)
replace("/etc/vm/cargo-config.toml", settings["cargo_config"], sensitive_mode, owner)
replace("/etc/vm/package-client.revision", settings["revision"] + "\n", sensitive_mode, owner)
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
"#;

const INSTALL_GUEST_CLIENT: &str = r#"set -eu
. /etc/profile.d/vm-packages.sh
url=${VM_PACKAGES_CLIENT_URL:?managed guest client URL unavailable}
token=${CARGO_REGISTRIES_VM_TOKEN:?package read token unavailable}
destination="$HOME/.local/bin/vm"
state="$HOME/.local/state/vm"
mkdir -p "$(dirname "$destination")" "$state"
task=$(mktemp -d "$state/client.XXXXXX")
cleanup() {
  rm -rf "$task"
}
trap cleanup EXIT HUP INT TERM
curl_args="--fail --silent --show-error --location --connect-timeout 5 --max-time 600 --retry 2"
printf 'header = "Authorization: Bearer %s"\n' "$token" > "$task/curl.conf"
chmod 0600 "$task/curl.conf"
if ! curl $curl_args --config "$task/curl.conf" \
    --output "$task/digest" "$url.sha256"; then
  if test -x "$destination"; then
    exit 0
  fi
  echo "Managed guest VM client is unavailable; run 'vm packages up' on the controller host" >&2
  exit 1
fi
expected=$(tr -d '[:space:]' < "$task/digest")
case "$expected" in
  *[!0-9a-f]*|'') echo "Managed guest VM client digest is invalid" >&2; exit 1 ;;
esac
test "${#expected}" -eq 64 || {
  echo "Managed guest VM client digest is invalid" >&2
  exit 1
}
installed=$(cat "$state/client.sha256" 2>/dev/null || true)
if test -x "$destination" && test "$installed" = "$expected"; then
  exit 0
fi
curl $curl_args --config "$task/curl.conf" \
  --output "$task/vm" "$url"
if command -v sha256sum >/dev/null 2>&1; then
  printf '%s  %s\n' "$expected" "$task/vm" | sha256sum -c - >/dev/null
elif command -v shasum >/dev/null 2>&1; then
  actual=$(shasum -a 256 "$task/vm" | awk '{print $1}')
  test "$actual" = "$expected"
else
  echo "No SHA-256 verifier is installed" >&2
  exit 1
fi
chmod 0755 "$task/vm"
"$task/vm" --version >/dev/null
mv -f "$task/vm" "$destination"
printf '%s\n' "$expected" > "$task/client.sha256"
mv -f "$task/client.sha256" "$state/client.sha256"
"#;

const RECONCILE_GIT_SETTING: &str = r#"current=$(git config --global --get "$1" 2>/dev/null || true)
if [ "$current" != "$2" ]; then
  git config --global "$1" "$2"
fi"#;

pub(super) trait PackageExecutor {
    fn checkout_root(&self, checkout_id: &str) -> VmResult<String>;
    fn workspace(&self) -> &str;
    fn run(&self, command: &[String]) -> VmResult<()>;
    fn output(&self, command: &[String]) -> VmResult<String>;
    fn write_private(&self, content: &[u8], destination: &str) -> VmResult<()>;
}

pub(super) struct GuestRuntime {
    consumer: String,
    gateway: String,
    agent_token: String,
    workspace: String,
    home: PathBuf,
}

impl GuestRuntime {
    pub(super) fn discover() -> VmResult<Self> {
        let consumer = required_guest_variable("VM_PACKAGES_CONSUMER")?;
        vm_packages::validate_label("consumer", &consumer).map_err(VmError::from)?;
        let gateway = required_guest_variable("VM_PACKAGES_WORK_GATEWAY")?;
        RegistryEndpoints::new(&gateway).map_err(VmError::from)?;
        let agent_token = required_guest_variable("VM_PACKAGES_AGENT_TOKEN")?;
        let workspace = std::env::current_dir()
            .map_err(VmError::from)?
            .to_string_lossy()
            .into_owned();
        let home = dirs::home_dir().ok_or_else(|| {
            VmError::validation("Guest home directory is unavailable", None::<String>)
        })?;
        Ok(Self {
            consumer,
            gateway,
            agent_token,
            workspace,
            home,
        })
    }

    pub(super) fn consumer(&self) -> &str {
        &self.consumer
    }

    pub(super) fn gateway(&self) -> &str {
        &self.gateway
    }

    pub(super) fn request_state_path(&self, key: &str) -> VmResult<PathBuf> {
        vm_packages::validate_managed_id("request key", key).map_err(VmError::from)?;
        Ok(self
            .home
            .join(".local/state/vm/package-requests")
            .join(format!("{key}.json")))
    }

    pub(super) fn client(&self) -> VmResult<vm_packages::PackageInfrastructureClient> {
        Ok(vm_packages::PackageInfrastructureClient::new(
            RegistryEndpoints::new(&self.gateway).map_err(VmError::from)?,
        )
        .with_agent_token(&self.agent_token))
    }
}

fn required_guest_variable(name: &str) -> VmResult<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            VmError::validation(
                format!("Managed guest package access is missing {name}"),
                Some("Run `vm tools update` on the controller host, then open a new guest shell"),
            )
        })
}

pub(in crate::commands) fn apply_client_environment(config: &mut VmConfig) -> VmResult<()> {
    let Some((client, edge)) = configured_client_environment(config)? else {
        return Ok(());
    };
    config.package_edge = Some(edge);
    apply_environment(config, &client);
    Ok(())
}

pub(in crate::commands) fn reconcile_client_settings(
    provider: &dyn Provider,
    environment: &str,
    config: &VmConfig,
) -> VmResult<()> {
    if !config.environment.contains_key("VM_PACKAGES_AGENT_TOKEN") {
        return Ok(());
    }
    let mut effective_config = config.clone();
    if let Ok(instances) = provider.list_instances() {
        if let Some(project) = instances
            .iter()
            .find(|instance| instance.name == environment)
            .and_then(|instance| instance.project.as_deref())
        {
            effective_config
                .project
                .get_or_insert_with(Default::default)
                .name = Some(project.to_string());
        }
    }
    let Some((client, _)) = configured_client_environment(&effective_config)? else {
        return Ok(());
    };
    let content = serde_json::to_vec(&client.managed_settings())
        .map_err(|error| VmError::general(error, "Failed to render package client settings"))?;
    let command = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "if [ \"$(id -u)\" -eq 0 ]; then exec python3 -c \"$1\"; else exec sudo -n python3 -c \"$1\"; fi".to_string(),
        "vm-package-settings".to_string(),
        INSTALL_CLIENT_SETTINGS.to_string(),
    ];
    provider
        .exec_with_stdin(Some(environment), &command, &content)
        .map_err(VmError::from)?;
    reconcile_git_identity(provider, environment, &effective_config)?;
    provider
        .exec(
            Some(environment),
            &[
                "/bin/sh".to_string(),
                "-c".to_string(),
                INSTALL_GUEST_CLIENT.to_string(),
            ],
        )
        .map_err(VmError::from)
}

fn reconcile_git_identity(
    provider: &dyn Provider,
    environment: &str,
    config: &VmConfig,
) -> VmResult<()> {
    for (key, value) in configured_git_identity(config)? {
        provider
            .exec(
                Some(environment),
                &[
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    RECONCILE_GIT_SETTING.to_string(),
                    "vm-git-identity".to_string(),
                    key.to_string(),
                    value,
                ],
            )
            .map_err(VmError::from)?;
    }
    Ok(())
}

fn configured_git_identity(config: &VmConfig) -> VmResult<Vec<(&'static str, String)>> {
    if config
        .host_sync
        .as_ref()
        .is_some_and(|host_sync| !host_sync.git_config)
    {
        return Ok(Vec::new());
    }
    let Some(git) = &config.git_config else {
        return Ok(Vec::new());
    };
    let mut settings = Vec::new();
    for (key, value) in [
        ("user.name", git.user_name.as_deref()),
        ("user.email", git.user_email.as_deref()),
    ] {
        let Some(value) = value else {
            continue;
        };
        if value.trim().is_empty() || value.contains(['\0', '\n', '\r']) {
            return Err(VmError::validation(
                format!("Host Git {key} must be one non-empty line"),
                Some("Fix the host Git configuration and rerun `vm tools update`"),
            ));
        }
        settings.push((key, value.to_string()));
    }
    Ok(settings)
}

fn configured_client_environment(
    config: &VmConfig,
) -> VmResult<Option<(ClientEnvironment, PackageEdgeConfig)>> {
    let files = ApplianceFiles::discover()?;
    let Some(state) = files.read_state()? else {
        return Ok(None);
    };
    let state = appliance::repair_client_access(&files, state)?;
    let provider = config.provider.as_deref().unwrap_or("docker");
    if provider == "tart"
        && (config.os.as_deref() == Some("macos")
            || config
                .tart
                .as_ref()
                .and_then(|tart| tart.guest_os.as_deref())
                == Some("macos"))
    {
        return Err(VmError::validation(
            "The managed package edge requires a Linux Tart guest",
            Some("Use the vibe-tart Linux profile"),
        ));
    }
    let consumer = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let (client, edge) = client_environment(
        &state,
        files.read_token()?,
        &files.agent_signing_key()?,
        consumer,
        provider,
    )?;
    Ok(Some((client, edge)))
}

fn apply_environment(config: &mut VmConfig, client: &ClientEnvironment) {
    config.environment.extend(client.variables());
}

fn client_environment(
    state: &ApplianceState,
    read_token: String,
    agent_signing_key: &str,
    consumer: &str,
    provider: &str,
) -> VmResult<(ClientEnvironment, PackageEdgeConfig)> {
    if state.registry_image.trim().is_empty() {
        return Err(VmError::validation(
            "Package appliance state predates worker-edge support",
            Some("Run `vm packages up` to refresh it"),
        ));
    }
    let internal_gateway = gateway_for_provider(state, provider)?;
    let client_gateway = match provider {
        "docker" | "podman" => "http://package-edge:3080".to_string(),
        "tart" => "http://127.0.0.1:3080".to_string(),
        _ => {
            return Err(VmError::validation(
                format!("Provider '{provider}' does not support the managed package edge"),
                Some("Use Docker, Podman, or a Linux Tart guest"),
            ))
        }
    };
    let revision = package_edge_revision(state, &internal_gateway, &read_token);
    let edge = PackageEdgeConfig {
        image: state.registry_image.clone(),
        internal_gateway: internal_gateway.clone(),
        client_gateway: client_gateway.clone(),
        read_token: read_token.clone(),
        revision,
    };
    let agent_token =
        vm_packages::issue_agent_capability(agent_signing_key, consumer).map_err(VmError::from)?;
    let client = ClientEnvironment::new(
        RegistryEndpoints::new(client_gateway).map_err(VmError::from)?,
        read_token,
    )
    .and_then(|client| client.with_oci_mirror(internal_gateway))
    .and_then(|client| client.with_agent_access(&edge.internal_gateway, agent_token, consumer))
    .map_err(VmError::from)?;
    Ok((client, edge))
}

fn package_edge_revision(
    state: &ApplianceState,
    internal_gateway: &str,
    read_token: &str,
) -> String {
    vm_packages::sha256_hex(format!(
        "{}\0{}\0{}\0{}\0{}\0{}",
        PACKAGE_EDGE_POLICY_REVISION,
        env!("CARGO_PKG_VERSION"),
        state.controller_version,
        state.registry_image,
        internal_gateway,
        read_token
    ))
}

pub(super) fn gateway_for_provider(state: &ApplianceState, provider: &str) -> VmResult<String> {
    match (state.runtime, provider) {
        (InfrastructureRuntime::Docker, "tart") => Err(VmError::validation(
            "A Docker-hosted package appliance is not reachable from Tart guests",
            Some("Run `vm packages up --runtime tart` so every environment can reach it"),
        )),
        (InfrastructureRuntime::Docker, "docker" | "podman") => Ok(format!(
            "http://{}:{}",
            vm_platform::platform::get_host_gateway(),
            state.gateway_port
        )),
        (InfrastructureRuntime::Docker, _) => Err(VmError::validation(
            format!("Provider '{provider}' cannot reach a Docker-hosted package appliance"),
            Some("Use the Tart package infrastructure runtime"),
        )),
        (InfrastructureRuntime::Tart, _) => Ok(state.gateway_url.clone()),
    }
}

pub(super) fn checkout_root(subject: &impl PackageExecutor, checkout_id: &str) -> VmResult<String> {
    subject.checkout_root(checkout_id)
}

fn checkout_root_for(config: &VmConfig, provider: &str, checkout_id: &str) -> VmResult<String> {
    vm_packages::validate_managed_id("checkout ID", checkout_id).map_err(VmError::from)?;
    let user = match provider {
        "tart" => config
            .tart
            .as_ref()
            .and_then(|tart| tart.ssh_user.as_deref())
            .unwrap_or("admin"),
        _ => config
            .vm
            .as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer"),
    };
    vm_packages::validate_label("guest user", user).map_err(VmError::from)?;
    let home = if user == "root" {
        "/root".to_string()
    } else if provider == "tart"
        && (config.os.as_deref() == Some("macos")
            || config
                .tart
                .as_ref()
                .and_then(|tart| tart.guest_os.as_deref())
                == Some("macos"))
    {
        format!("/Users/{user}")
    } else {
        format!("/home/{user}")
    };
    Ok(format!(
        "{home}/.local/share/vm/package-checkouts/{checkout_id}"
    ))
}

pub(super) fn exec<I, S>(subject: &impl PackageExecutor, command: I) -> VmResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let command = command.into_iter().map(Into::into).collect::<Vec<_>>();
    subject.run(&command)
}

pub(super) fn exec_output<I, S>(subject: &impl PackageExecutor, command: I) -> VmResult<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let command = command.into_iter().map(Into::into).collect::<Vec<_>>();
    subject.output(&command)
}

pub(super) fn copy_private(
    subject: &impl PackageExecutor,
    content: &[u8],
    destination: &str,
) -> VmResult<()> {
    subject.write_private(content, destination)
}

pub(super) fn exec_in_workspace<I, S>(subject: &impl PackageExecutor, command: I) -> VmResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut wrapped = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "cd \"$1\"; shift; exec \"$@\"".to_string(),
        "vm-package-workspace".to_string(),
        subject.workspace().to_string(),
    ];
    wrapped.extend(command.into_iter().map(Into::into));
    subject.run(&wrapped)
}

fn workspace_path(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace")
}

impl PackageExecutor for RuntimeSubject {
    fn checkout_root(&self, checkout_id: &str) -> VmResult<String> {
        checkout_root_for(&self.config, self.provider.name(), checkout_id)
    }

    fn workspace(&self) -> &str {
        workspace_path(&self.config)
    }

    fn run(&self, command: &[String]) -> VmResult<()> {
        self.provider
            .exec(Some(self.target.as_str()), command)
            .map_err(VmError::from)
    }

    fn output(&self, command: &[String]) -> VmResult<String> {
        self.provider
            .exec_output(Some(self.target.as_str()), command)
            .map_err(VmError::from)
    }

    fn write_private(&self, content: &[u8], destination: &str) -> VmResult<()> {
        let mut temporary = tempfile::NamedTempFile::new().map_err(VmError::from)?;
        temporary.write_all(content).map_err(VmError::from)?;
        temporary.flush().map_err(VmError::from)?;
        self.provider
            .copy(
                &temporary.path().to_string_lossy(),
                destination,
                Some(self.target.as_str()),
            )
            .map_err(VmError::from)?;
        self.run(&["chmod".into(), "600".into(), destination.into()])
    }
}

impl PackageExecutor for GuestRuntime {
    fn checkout_root(&self, checkout_id: &str) -> VmResult<String> {
        vm_packages::validate_managed_id("checkout ID", checkout_id).map_err(VmError::from)?;
        Ok(self
            .home
            .join(".local/share/vm/package-checkouts")
            .join(checkout_id)
            .to_string_lossy()
            .into_owned())
    }

    fn workspace(&self) -> &str {
        &self.workspace
    }

    fn run(&self, command: &[String]) -> VmResult<()> {
        let (program, arguments) = command.split_first().ok_or_else(|| {
            VmError::validation("Package command cannot be empty", None::<String>)
        })?;
        let status = Command::new(program)
            .args(arguments)
            .stdin(Stdio::null())
            .status()
            .map_err(VmError::from)?;
        if status.success() {
            Ok(())
        } else {
            Err(VmError::validation(
                format!("Package command failed with {status}: {program}"),
                None::<String>,
            ))
        }
    }

    fn output(&self, command: &[String]) -> VmResult<String> {
        let (program, arguments) = command.split_first().ok_or_else(|| {
            VmError::validation("Package command cannot be empty", None::<String>)
        })?;
        let output = Command::new(program)
            .args(arguments)
            .stdin(Stdio::null())
            .output()
            .map_err(VmError::from)?;
        if !output.status.success() {
            return Err(VmError::validation(
                format!("Package command failed with {}: {program}", output.status),
                None::<String>,
            ));
        }
        String::from_utf8(output.stdout)
            .map_err(|error| VmError::general(error, "Package command returned non-UTF-8 output"))
    }

    fn write_private(&self, content: &[u8], destination: &str) -> VmResult<()> {
        let destination = Path::new(destination);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(VmError::from)?;
        }
        vm_core::file_system::atomic_write(destination, content).map_err(VmError::from)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o600))
                .map_err(VmError::from)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_environment, checkout_root_for, client_environment, configured_git_identity,
        gateway_for_provider, package_edge_revision, INSTALL_CLIENT_SETTINGS, INSTALL_GUEST_CLIENT,
        RECONCILE_GIT_SETTING,
    };
    use vm_config::config::{HostSyncConfig, TartConfig, VmConfig, VmSettings};
    use vm_config::detector::git::GitConfig;
    use vm_packages::{
        ApplianceState, ClientEnvironment, InfrastructureRuntime, RegistryEndpoints,
    };

    fn state(runtime: InfrastructureRuntime) -> ApplianceState {
        ApplianceState {
            definition_revision: vm_packages::APPLIANCE_DEFINITION_REVISION,
            runtime,
            gateway_url: "http://192.0.2.8:3080".into(),
            gateway_port: 3080,
            registry_image: "registry/image:1".into(),
            job_image: "jobs/image:1".into(),
            controller_version: "1".into(),
            tart_home: None,
        }
    }

    #[test]
    fn tart_appliance_has_one_provider_neutral_client_shape() {
        let state = state(InfrastructureRuntime::Tart);
        let signing_key = "agent-signing-key-012345678901234567890123456789";
        let (docker, docker_edge) = client_environment(
            &state,
            "read-token".into(),
            signing_key,
            "project-a",
            "docker",
        )
        .unwrap();
        let (tart, tart_edge) = client_environment(
            &state,
            "read-token".into(),
            signing_key,
            "project-a",
            "tart",
        )
        .unwrap();
        let docker_variables = docker
            .variables()
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        let tart_variables = tart
            .variables()
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(docker_variables["NPM_CONFIG_REGISTRY"].contains("package-edge"));
        assert!(tart_variables["NPM_CONFIG_REGISTRY"].contains("127.0.0.1"));
        assert_eq!(docker_variables["VM_OCI_MIRROR"], state.gateway_url);
        assert_eq!(docker_variables["VM_PACKAGES_CONSUMER"], "project-a");
        assert_eq!(
            docker_variables["VM_PACKAGES_WORK_GATEWAY"],
            state.gateway_url
        );
        assert_eq!(tart_variables["VM_OCI_MIRROR"], state.gateway_url);
        assert_eq!(docker_edge.client_gateway, "http://package-edge:3080");
        assert_eq!(tart_edge.client_gateway, "http://127.0.0.1:3080");
        assert_eq!(docker_edge.internal_gateway, state.gateway_url);
    }

    #[test]
    fn edge_revision_tracks_controller_and_runtime_policy() {
        let mut first = state(InfrastructureRuntime::Tart);
        let initial = package_edge_revision(&first, &first.gateway_url, "read-token");

        first.controller_version = "2".into();
        let upgraded = package_edge_revision(&first, &first.gateway_url, "read-token");

        assert_ne!(initial, upgraded);
        assert_eq!(
            upgraded,
            package_edge_revision(&first, &first.gateway_url, "read-token")
        );
    }

    #[test]
    fn tart_guests_reject_a_loopback_only_docker_appliance() {
        assert!(gateway_for_provider(&state(InfrastructureRuntime::Docker), "tart").is_err());
    }

    #[test]
    fn managed_package_settings_replace_project_redirects() {
        let endpoints = RegistryEndpoints::new("http://packages.internal:3080").unwrap();
        let client = ClientEnvironment::new(endpoints, "read-token").unwrap();
        let mut config = VmConfig::default();
        config.environment.insert(
            "NPM_CONFIG_REGISTRY".into(),
            "https://untrusted.invalid".into(),
        );

        apply_environment(&mut config, &client);

        assert!(config.environment["NPM_CONFIG_REGISTRY"].contains("packages.internal"));
        assert_eq!(
            config.environment["CARGO_SOURCE_CRATES_IO_REPLACE_WITH"],
            "vm"
        );
    }

    #[test]
    fn client_repair_is_atomic_and_sources_interactive_shells() {
        assert!(INSTALL_CLIENT_SETTINGS.contains("os.replace(temporary, path)"));
        assert!(INSTALL_CLIENT_SETTINGS.contains("/etc/profile.d/vm-packages.sh"));
        assert!(INSTALL_CLIENT_SETTINGS.contains("/etc/bash.bashrc"));
        assert!(INSTALL_CLIENT_SETTINGS.contains("/etc/zsh/zshrc"));
        assert!(INSTALL_CLIENT_SETTINGS.contains("/etc/vm/managed-guest"));
        assert!(INSTALL_CLIENT_SETTINGS.contains("refusing managed file symlink"));
    }

    #[test]
    fn guest_client_repair_is_authenticated_verified_and_atomic() {
        assert!(INSTALL_GUEST_CLIENT.contains("VM_PACKAGES_CLIENT_URL"));
        assert!(INSTALL_GUEST_CLIENT.contains("--config \"$task/curl.conf\""));
        assert!(INSTALL_GUEST_CLIENT.contains("sha256sum -c"));
        assert!(INSTALL_GUEST_CLIENT.contains("mv -f \"$task/vm\" \"$destination\""));
        assert!(!INSTALL_GUEST_CLIENT.contains("--header \"Authorization"));
    }

    #[test]
    fn git_identity_reconciliation_is_idempotent_and_honors_host_sync() {
        let mut config = VmConfig {
            git_config: Some(GitConfig {
                user_name: Some("Agent User".into()),
                user_email: Some("agent@example.test".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            configured_git_identity(&config).unwrap(),
            [
                ("user.name", "Agent User".to_string()),
                ("user.email", "agent@example.test".to_string())
            ]
        );
        assert!(RECONCILE_GIT_SETTING.contains("current=$(git config --global --get"));
        assert!(RECONCILE_GIT_SETTING.contains("if [ \"$current\" != \"$2\" ]"));

        config.host_sync = Some(HostSyncConfig {
            git_config: false,
            ..Default::default()
        });
        assert!(configured_git_identity(&config).unwrap().is_empty());
    }

    #[test]
    fn git_identity_rejects_multiline_values() {
        let config = VmConfig {
            git_config: Some(GitConfig {
                user_name: Some("Agent\nInjected".into()),
                user_email: Some("agent@example.test".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(configured_git_identity(&config).is_err());
    }

    #[test]
    fn checkout_roots_cannot_escape_guest_temporary_storage() {
        let docker = VmConfig {
            vm: Some(VmSettings {
                user: Some("developer".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            checkout_root_for(&docker, "docker", "pkg-auth-20260811-000001").unwrap(),
            "/home/developer/.local/share/vm/package-checkouts/pkg-auth-20260811-000001"
        );
        for invalid in ["../workspace", "/workspace", "scope/auth", "."] {
            assert!(checkout_root_for(&docker, "docker", invalid).is_err());
        }

        let tart = VmConfig {
            tart: Some(TartConfig {
                ssh_user: Some("admin".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            checkout_root_for(&tart, "tart", "pkg-auth-20260811-000001").unwrap(),
            "/home/admin/.local/share/vm/package-checkouts/pkg-auth-20260811-000001"
        );
    }
}
