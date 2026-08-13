use super::{GuestCommand, TartProvisioner};
use crate::project_plan::{NodeToolchainPlan, PrimaryRuntime, ProjectPlan};
use crate::shell_session::quote_posix_argument;
use tracing::{info, warn};
use vm_config::config::{MountAccess, VmConfig};
use vm_core::vm_warning;

impl TartProvisioner {
    pub(crate) fn reconcile_runtime(&self, config: &VmConfig) -> vm_core::error::Result<()> {
        self.apply_shell_config(config)?;
        let Some(edge_command) = Self::package_edge_command(config) else {
            return Ok(());
        };
        let mut commands = Vec::new();
        if let Some(docker_command) = self.docker_install_command(config) {
            commands.push((
                "Docker runtime",
                format!("if ! command -v docker >/dev/null 2>&1; then\n{docker_command}\nfi"),
            ));
        }
        commands.push(("package edge", edge_command));
        self.ssh_exec_batch(commands)
    }

    pub(super) fn guest_software_commands(
        &self,
        config: &VmConfig,
        project_plan: &ProjectPlan,
    ) -> Vec<GuestCommand> {
        let runtime = project_plan.primary_runtime();
        info!("Detected framework: {}", runtime.as_str());
        let mut commands = Vec::new();

        if !config.apt_packages.is_empty() {
            let packages = Self::shell_quote_packages(&config.apt_packages);
            let command = if self.is_macos_guest(config) {
                format!("{}\nbrew install {packages}", Self::homebrew_preamble())
            } else {
                format!("sudo apt-get update && sudo apt-get install -y {packages}")
            };
            commands.push(("generic system packages", command));
        }

        if let Some(command) = self.docker_install_command(config) {
            commands.push(("Docker runtime", command));
        }
        if let Some(command) = Self::package_edge_command(config) {
            commands.push(("package edge", command));
        }

        if let Some(node) = project_plan.installs.node.as_ref() {
            commands.push(("Node.js toolchain", Self::node_toolchain_command(node)));
        }
        if !config.npm_packages.is_empty() {
            commands.push((
                "global npm packages",
                format!(
                    r#"export PATH="{}"
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
npm install -g {}"#,
                    Self::user_bin_path(config),
                    Self::shell_quote_packages(&config.npm_packages)
                ),
            ));
        }

        if !config.pip_packages.is_empty() || runtime == PrimaryRuntime::Python {
            commands.push((
                "Python runtime and packages",
                self.python_runtime_command(config),
            ));
        }

        if !config.cargo_packages.is_empty() || runtime == PrimaryRuntime::Rust {
            commands.push((
                "Rust runtime and packages",
                Self::rust_runtime_command(config),
            ));
        }

        match runtime {
            PrimaryRuntime::Node => {
                if let Some(command) = self.node_bootstrap_command(project_plan) {
                    commands.push(("Node.js project dependencies", command));
                }
            }
            PrimaryRuntime::Python => commands.push((
                "Python project dependencies",
                self.python_project_command(config),
            )),
            PrimaryRuntime::Ruby => commands.push((
                "Ruby project dependencies",
                self.ruby_project_command(config),
            )),
            PrimaryRuntime::Rust => commands.push((
                "Rust project dependencies",
                Self::rust_project_command(&self.project_dir),
            )),
            PrimaryRuntime::Go => {
                commands.push(("Go project dependencies", self.go_project_command(config)))
            }
            PrimaryRuntime::Unknown => warn!("Unknown framework, skipping project dependencies"),
        }

        if let Some(command) = self.database_command(config) {
            commands.push(("database services", command));
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
current="$(sudo docker inspect --format '{{{{ index .Config.Labels "com.vm.package-edge.revision" }}}}' vm-package-edge 2>/dev/null || true)"
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
until [ "$(sudo docker inspect --format '{{{{.State.Health.Status}}}}' vm-package-edge 2>/dev/null || true)" = healthy ]; do
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

    fn node_toolchain_command(node: &NodeToolchainPlan) -> String {
        let node_version = quote_posix_argument(&node.node);
        let nvm_version = quote_posix_argument(&node.nvm);
        let npm_version = quote_posix_argument(node.npm.as_deref().unwrap_or(""));
        let pnpm_version = quote_posix_argument(&node.pnpm);

        format!(
            r#"set -euo pipefail
export VM_NODE_VERSION={node_version}
export VM_NVM_VERSION={nvm_version}
export VM_NPM_VERSION={npm_version}
export VM_PNPM_VERSION={pnpm_version}
installer="$(mktemp)"
trap 'rm -f "$installer"' EXIT
cat > "$installer" <<'VM_NODE_TOOLCHAIN'
{}
VM_NODE_TOOLCHAIN
bash "$installer""#,
            crate::resources::NODE_TOOLCHAIN_INSTALLER
        )
    }

    fn node_bootstrap_command(&self, project_plan: &ProjectPlan) -> Option<String> {
        let manager = project_plan
            .installs
            .node_dependencies
            .map_or("", |manager| manager.as_str());
        let browsers = project_plan.installs.playwright_browsers.join(" ");
        if manager.is_empty() && browsers.is_empty() {
            return None;
        }

        let project_dir = quote_posix_argument(&self.project_dir);
        let manager = quote_posix_argument(manager);
        let browsers = quote_posix_argument(&browsers);
        Some(format!(
            r#"set -euo pipefail
export VM_PROJECT_PATH={project_dir}
export VM_NODE_DEPENDENCY_MANAGER={manager}
export VM_PLAYWRIGHT_BROWSERS={browsers}
bootstrap="$(mktemp)"
trap 'rm -f "$bootstrap"' EXIT
cat > "$bootstrap" <<'VM_NODE_BOOTSTRAP'
{}
VM_NODE_BOOTSTRAP
bash "$bootstrap""#,
            crate::resources::NODE_BOOTSTRAP
        ))
    }

    fn python_runtime_command(&self, config: &VmConfig) -> String {
        let python_version = config
            .versions
            .as_ref()
            .and_then(|versions| versions.python.as_deref())
            .unwrap_or("3.11");
        let python_version = quote_posix_argument(python_version);
        let tooling = if self.is_macos_guest(config) {
            format!(
                "{}\nif ! command -v pipx >/dev/null 2>&1; then brew install pipx; fi",
                Self::homebrew_preamble()
            )
        } else {
            r#"if ! command -v pipx >/dev/null 2>&1; then
  sudo apt-get update
  sudo apt-get install -y pipx python3-pip python3-venv
fi"#
            .to_string()
        };
        let packages = if config.pip_packages.is_empty() {
            String::new()
        } else {
            format!(
                "python3 -m pip install --user {} {}",
                if self.is_macos_guest(config) {
                    ""
                } else {
                    "--break-system-packages"
                },
                Self::shell_quote_packages(&config.pip_packages)
            )
        };
        format!(
            r#"export PATH="$HOME/.pyenv/bin:{}"
if ! command -v pyenv >/dev/null 2>&1; then
  curl -fsSL https://pyenv.run | bash
fi
eval "$(pyenv init -)"
pyenv install -s {python_version}
pyenv global {python_version}
{tooling}
export PATH="{}"
pipx ensurepath >/dev/null 2>&1 || true
{packages}"#,
            Self::user_bin_path(config),
            Self::user_bin_path(config)
        )
    }

    fn python_project_command(&self, config: &VmConfig) -> String {
        let project = quote_posix_argument(&self.project_dir);
        let venv = if config
            .project
            .as_ref()
            .is_some_and(|project| project.workspace_access == MountAccess::ReadOnly)
        {
            format!("$HOME/.local/share/vm/venvs/{}", self.instance_name)
        } else {
            format!("{}/.venv", self.project_dir)
        };
        let venv = crate::shell_session::quote_posix_home_path(&venv);
        format!(
            r#"export PATH="$HOME/.pyenv/bin:{}"
eval "$(pyenv init -)"
if [ -f {project}/requirements.txt ]; then
  venv={venv}
  if [ ! -d "$venv" ]; then
    mkdir -p "$(dirname "$venv")"
    python3 -m venv "$venv"
  fi
  . "$venv/bin/activate"
  pip install -r {project}/requirements.txt
fi"#,
            Self::user_bin_path(config)
        )
    }

    fn ruby_project_command(&self, config: &VmConfig) -> String {
        let project = quote_posix_argument(&self.project_dir);
        format!(
            r#"{}
if [ -f {project}/Gemfile ]; then
  if ! command -v bundle >/dev/null 2>&1; then gem install bundler; fi
  bundle config set --global path "$HOME/.local/share/vm/bundle"
  if [ -f {project}/Gemfile.lock ]; then
    cd {project} && bundle config set --global frozen true && bundle install
  else
    cd {project} && bundle install
  fi
fi"#,
            if self.is_macos_guest(config) {
                format!(
                    "{}\nif ! command -v ruby >/dev/null 2>&1; then brew install ruby; fi",
                    Self::homebrew_preamble()
                )
            } else {
                "sudo apt-get update && sudo apt-get install -y ruby-full build-essential zlib1g-dev"
                    .to_string()
            }
        )
    }

    fn rust_runtime_command(config: &VmConfig) -> String {
        let packages = if config.cargo_packages.is_empty() {
            String::new()
        } else {
            format!(
                "cargo install {}",
                Self::shell_quote_packages(&config.cargo_packages)
            )
        };
        format!(
            r#"if [ ! -x "$HOME/.cargo/bin/cargo" ]; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi
export PATH="$HOME/.cargo/bin:$PATH"
if ! rustup show active-toolchain 2>/dev/null | grep -q '^stable-'; then
  rustup default stable
fi
{packages}"#
        )
    }

    fn rust_project_command(project_dir: &str) -> String {
        let project = quote_posix_argument(project_dir);
        format!(
            r#"export PATH="$HOME/.cargo/bin:$PATH"
if [ -f {project}/Cargo.toml ]; then
  if [ -f {project}/Cargo.lock ]; then
    cd {project} && cargo fetch --locked
  else
    cd {project} && cargo fetch
  fi
fi"#
        )
    }

    fn go_project_command(&self, config: &VmConfig) -> String {
        let project = quote_posix_argument(&self.project_dir);
        format!(
            r#"if ! command -v go >/dev/null 2>&1; then
  {}
fi
if [ -f {project}/go.mod ]; then
  cd {project} && GOFLAGS=-mod=readonly go mod download
fi"#,
            if self.is_macos_guest(config) {
                format!("{}\nbrew install go", Self::homebrew_preamble())
            } else {
                "sudo apt-get update && sudo apt-get install -y golang-go".to_string()
            }
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
