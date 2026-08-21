use super::{GuestCommand, TartProvisioner};
use crate::shell_session::quote_posix_argument;
use vm_config::config::VmConfig;
use vm_core::vm_warning;

impl TartProvisioner {
    pub(crate) fn reconcile_runtime(&self, config: &VmConfig) -> vm_core::error::Result<()> {
        self.apply_shell_config(config)?;
        if config.package_edge.is_none() {
            return Ok(());
        }
        self.ssh_exec_batch(self.package_infrastructure_commands(config))
    }

    pub(super) fn package_infrastructure_commands(&self, config: &VmConfig) -> Vec<GuestCommand> {
        let mut commands = Vec::new();
        if let Some(docker_command) = self.docker_install_command(config) {
            commands.push((
                "Docker runtime",
                format!("if ! command -v docker >/dev/null 2>&1; then\n{docker_command}\nfi"),
            ));
        }
        if let Some(edge_command) = Self::package_edge_command(config) {
            commands.push(("package edge", edge_command));
        }
        commands
    }

    fn docker_install_command(&self, config: &VmConfig) -> Option<String> {
        if config.package_edge.is_none()
            && !config
                .tart
                .as_ref()
                .and_then(|tart| tart.install_docker)
                .unwrap_or(false)
        {
            return None;
        }

        if self.is_macos_guest(config) {
            vm_warning!(
                "Docker in a macOS Tart guest uses Colima with QEMU software emulation and will be much slower. Prefer the Linux Tart profile for Docker workloads."
            );
            return Some(self.macos_docker_tools_command());
        }

        let mirror = config
            .environment
            .get("VM_OCI_MIRROR")
            .map(|value| {
                let value = quote_posix_argument(value);
                format!(
                    r#"if command -v python3 >/dev/null 2>&1; then
  sudo python3 - {value} <<'PY'
import json
import os
import pathlib
import sys

path = pathlib.Path("/etc/docker/daemon.json")
content = path.read_text().strip() if path.exists() else ""
config = json.loads(content) if content else {{}}
mirrors = config.setdefault("registry-mirrors", [])
if not isinstance(mirrors, list):
    raise SystemExit("/etc/docker/daemon.json registry-mirrors must be a list")
if sys.argv[1] not in mirrors:
    mirrors.append(sys.argv[1])
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".json.vm-tmp")
    temporary.write_text(json.dumps(config, indent=2) + "\n")
    os.chmod(temporary, 0o644)
    os.replace(temporary, path)
PY
else
  printf '%s\n' 'python3 is unavailable; skipping Docker registry mirror activation' >&2
fi"#
                )
            })
            .unwrap_or_default();
        let restart = if mirror.is_empty() {
            ""
        } else {
            r#"if command -v systemctl >/dev/null 2>&1; then
  sudo systemctl restart docker >/dev/null 2>&1
elif command -v service >/dev/null 2>&1; then
  sudo service docker restart >/dev/null 2>&1
fi"#
        };

        Some(format!(
            r#"if ! command -v docker >/dev/null 2>&1; then
  curl -fsSL https://get.docker.com | sh
  if command -v sudo >/dev/null 2>&1; then
    sudo usermod -aG docker "$USER" || true
  fi
fi
{mirror}
if command -v systemctl >/dev/null 2>&1; then
  sudo systemctl enable --now docker >/dev/null 2>&1 || true
elif command -v service >/dev/null 2>&1; then
  sudo service docker start >/dev/null 2>&1 || true
fi
{restart}
docker info >/dev/null 2>&1 || sudo docker info >/dev/null 2>&1"#
        ))
    }

    pub(crate) fn package_edge_command(config: &VmConfig) -> Option<String> {
        let edge = config.package_edge.as_ref()?;
        let image = quote_posix_argument(&edge.image);
        let gateway = quote_posix_argument(&format!(
            "PKG_SERVER_INTERNAL_GATEWAY={}",
            edge.internal_gateway
        ));
        let internal_token =
            quote_posix_argument(&format!("PKG_SERVER_INTERNAL_TOKEN={}", edge.read_token));
        let read_token =
            quote_posix_argument(&format!("PKG_SERVER_READ_TOKEN={}", edge.read_token));
        let revision = quote_posix_argument(&edge.revision);

        Some(format!(
            r#"set -e
sudo install -d -m 0700 /etc/vm
printf '%s\n' {gateway} {internal_token} {read_token} | sudo tee /etc/vm/package-edge.env >/dev/null
sudo chmod 0600 /etc/vm/package-edge.env
current="$(sudo docker container inspect --format '{{{{ index .Config.Labels "com.vm.package-edge.revision" }}}}' vm-package-edge 2>/dev/null || true)"
if [ "$current" != {revision} ]; then
  sudo docker rm -f vm-package-edge >/dev/null 2>&1 || true
  sudo docker run --detach \
    --name vm-package-edge \
    --restart unless-stopped \
    --network host \
    --read-only \
    --tmpfs /tmp:rw,nosuid,size=64m \
    --cap-drop ALL \
    --security-opt no-new-privileges:true \
    --label com.vm.managed=true \
    --label com.vm.package-edge.revision={revision} \
    --env-file /etc/vm/package-edge.env \
    --volume vm-package-edge-cache:/data \
    {image} >/dev/null
else
  sudo docker start vm-package-edge >/dev/null
fi
attempt=0
until [ "$(sudo docker container inspect --format '{{{{.State.Health.Status}}}}' vm-package-edge 2>/dev/null || true)" = healthy ]; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 40 ]; then
    sudo docker logs --tail 20 vm-package-edge >&2 || true
    exit 1
  fi
  sleep 0.25
done"#
        ))
    }

    fn macos_docker_tools_command(&self) -> String {
        let workspace = quote_posix_argument(&self.project_dir);
        format!(
            r#"{brew}
brew install docker docker-compose docker-buildx colima qemu

mkdir -p "$HOME/.docker"
if command -v python3 >/dev/null 2>&1; then
  python3 - "$HOME/.docker/config.json" <<'PY'
import json
import os
import sys

path = sys.argv[1]
config = {{}}
if os.path.exists(path):
    try:
        with open(path, "r", encoding="utf-8") as fh:
            config = json.load(fh)
    except Exception:
        config = {{}}
dirs = config.setdefault("cliPluginsExtraDirs", [])
plugin_dir = "/opt/homebrew/lib/docker/cli-plugins"
if plugin_dir not in dirs:
    dirs.append(plugin_dir)
with open(path, "w", encoding="utf-8") as fh:
    json.dump(config, fh, indent=2)
    fh.write("\n")
PY
else
  cat >"$HOME/.docker/config.json" <<'JSON'
{{
  "cliPluginsExtraDirs": [
    "/opt/homebrew/lib/docker/cli-plugins"
  ]
}}
JSON
fi

mkdir -p {workspace}
cat >{workspace}/qemu-system-aarch64-tcg <<'SH'
#!/bin/bash
args=()
for arg in "$@"; do
  case "$arg" in
    virt,accel=hvf)
      arg="virt,accel=tcg"
      ;;
    host)
      arg="max"
      ;;
  esac
  args+=("$arg")
done
exec /opt/homebrew/bin/qemu-system-aarch64 "${{args[@]}}"
SH
chmod +x {workspace}/qemu-system-aarch64-tcg

mkdir -p "$HOME/.local/share/qemu"
ln -sf /opt/homebrew/share/qemu/edk2-aarch64-code.fd "$HOME/.local/share/qemu/edk2-aarch64-code.fd"
ln -sf /opt/homebrew/share/qemu/edk2-aarch64-code.fd "$HOME/.local/share/qemu/edk-aarch64-tcg-code.fd"

cat >{workspace}/start-colima <<'SH'
#!/bin/sh
QEMU_SYSTEM_AARCH64="$(dirname "$0")/qemu-system-aarch64-tcg" \
  colima start --cpu 2 --memory 4 --disk 20 --vm-type qemu --cpu-type max
SH
chmod +x {workspace}/start-colima
"#,
            brew = Self::homebrew_preamble(),
            workspace = workspace
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tart::TartCommand;
    use vm_config::config::{PackageEdgeConfig, TartConfig};

    #[test]
    fn linux_docker_activates_the_managed_oci_mirror() {
        let provisioner = TartProvisioner::new(
            "vm-linux".to_string(),
            "/workspace".to_string(),
            TartCommand::new(None),
        );
        let mut config = VmConfig {
            os: Some("linux".to_string()),
            tart: Some(TartConfig {
                install_docker: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        config.environment.insert(
            "VM_OCI_MIRROR".into(),
            "http://packages.internal:3080".into(),
        );

        let command = provisioner.docker_install_command(&config).unwrap();

        assert!(command.contains("registry-mirrors"));
        assert!(command.contains("http://packages.internal:3080"));
        assert!(command.contains("os.replace"));
        assert!(command.contains("restart docker"));
        #[cfg(unix)]
        assert!(std::process::Command::new("/bin/bash")
            .args(["-n", "-c", &command])
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn linux_package_edge_is_private_persistent_and_idempotent() {
        let mut config = VmConfig {
            os: Some("linux".to_string()),
            package_edge: Some(PackageEdgeConfig {
                image: "registry.example/edge:1".into(),
                internal_gateway: "http://192.0.2.8:3080".into(),
                client_gateway: "http://127.0.0.1:3080".into(),
                read_token: "reader-token".into(),
                revision: "abc123".into(),
            }),
            ..Default::default()
        };
        let command = TartProvisioner::package_edge_command(&config).unwrap();

        assert!(command.contains("--network host"));
        assert!(command.contains("--read-only"));
        assert!(command.contains("--cap-drop ALL"));
        assert!(command.contains("vm-package-edge-cache:/data"));
        assert!(command.contains("chmod 0600 /etc/vm/package-edge.env"));
        assert!(command.contains("com.vm.package-edge.revision="));
        assert!(command.contains("abc123"));
        assert!(command.contains("sudo docker rm -f vm-package-edge"));
        assert!(!command.contains("docker volume rm"));
        assert!(!command.contains("PKG_SERVER_PUBLISH_TOKEN"));
        #[cfg(unix)]
        assert!(std::process::Command::new("/bin/bash")
            .args(["-n", "-c", &command])
            .status()
            .unwrap()
            .success());

        config.package_edge = None;
        assert!(TartProvisioner::package_edge_command(&config).is_none());
    }
}
