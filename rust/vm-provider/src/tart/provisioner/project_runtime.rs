use super::{GuestCommand, TartProvisioner};
use crate::project_plan::{NodeToolchainPlan, PrimaryRuntime, ProjectPlan};
use crate::shell_session::quote_posix_argument;
use tracing::{info, warn};
use vm_config::config::{MountAccess, VmConfig};

impl TartProvisioner {
    pub(super) fn project_runtime_commands(
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
