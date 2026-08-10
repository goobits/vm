use super::TartProvisioner;
use crate::project_plan::{PrimaryRuntime, ProjectPlan};
use tracing::{info, warn};
use vm_config::config::VmConfig;
use vm_core::error::Result;
use vm_core::vm_warning;

impl TartProvisioner {
    pub(super) fn provision_generic_packages(&self, config: &VmConfig) -> Result<()> {
        if !config.apt_packages.is_empty() {
            let packages = Self::shell_quote_packages(&config.apt_packages);
            if self.is_macos_guest(config) {
                self.ensure_homebrew()?;
                self.ssh_exec(&format!(
                    "{}\nbrew install {}",
                    Self::macos_brew_preamble(),
                    packages
                ))?;
            } else {
                self.ssh_exec(&format!(
                    "sudo apt-get update && sudo apt-get install -y {}",
                    packages
                ))?;
            }
        }

        if !config.npm_packages.is_empty() {
            self.ensure_nodejs_runtime(config)?;
            let packages = Self::shell_quote_packages(&config.npm_packages);
            self.ssh_exec(&format!(
                r#"export PATH="{}"
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
npm install -g {}"#,
                Self::user_bin_path(config),
                packages
            ))?;
        }

        if !config.pip_packages.is_empty() {
            self.ensure_python_runtime(config)?;
            self.ensure_python_package_tooling(config)?;
            let packages = Self::shell_quote_packages(&config.pip_packages);
            self.ssh_exec(&format!(
                r#"export PATH="{}"
python3 -m pip install --user {} {}"#,
                Self::user_bin_path(config),
                if self.is_macos_guest(config) {
                    ""
                } else {
                    "--break-system-packages"
                },
                packages
            ))?;
        }

        if !config.cargo_packages.is_empty() {
            self.ensure_rust_runtime()?;
            let packages = Self::shell_quote_packages(&config.cargo_packages);
            self.ssh_exec(&format!(
                r#"export PATH="$HOME/.cargo/bin:$PATH"
cargo install {}"#,
                packages
            ))?;
        }

        Ok(())
    }

    pub(super) fn install_docker_if_requested(&self, config: &VmConfig) -> Result<()> {
        if !config
            .tart
            .as_ref()
            .and_then(|tart| tart.install_docker)
            .unwrap_or(false)
        {
            return Ok(());
        }

        if self.is_macos_guest(config) {
            vm_warning!(
                "Docker in a macOS Tart guest uses Colima with QEMU software emulation and will be much slower. Prefer the Linux Tart profile for Docker workloads."
            );
            return self.install_macos_docker_tools();
        }

        self.ssh_exec(
            r#"if ! command -v docker >/dev/null 2>&1; then
  curl -fsSL https://get.docker.com | sh
  if command -v sudo >/dev/null 2>&1; then
    sudo usermod -aG docker "$USER" || true
  fi
fi
if command -v systemctl >/dev/null 2>&1; then
  sudo systemctl enable --now docker >/dev/null 2>&1 || true
elif command -v service >/dev/null 2>&1; then
  sudo service docker start >/dev/null 2>&1 || true
fi
docker info >/dev/null 2>&1 || sudo docker info >/dev/null 2>&1"#,
        )?;

        Ok(())
    }

    fn install_macos_docker_tools(&self) -> Result<()> {
        self.ensure_homebrew()?;

        let workspace = Self::shell_escape_single_quotes(&self.project_dir);
        self.ssh_exec(&format!(
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

mkdir -p '{workspace}'
cat >'{workspace}/qemu-system-aarch64-tcg' <<'SH'
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
chmod +x '{workspace}/qemu-system-aarch64-tcg'

mkdir -p "$HOME/.local/share/qemu"
ln -sf /opt/homebrew/share/qemu/edk2-aarch64-code.fd "$HOME/.local/share/qemu/edk2-aarch64-code.fd"
ln -sf /opt/homebrew/share/qemu/edk2-aarch64-code.fd "$HOME/.local/share/qemu/edk-aarch64-tcg-code.fd"

cat >'{workspace}/start-colima' <<'SH'
#!/bin/sh
QEMU_SYSTEM_AARCH64="$(dirname "$0")/qemu-system-aarch64-tcg" \
  colima start --cpu 2 --memory 4 --disk 20 --vm-type qemu --cpu-type max
SH
chmod +x '{workspace}/start-colima'
"#,
            brew = Self::macos_brew_preamble(),
            workspace = workspace
        ))?;

        Ok(())
    }

    pub(super) fn provision_framework_dependencies(
        &self,
        config: &VmConfig,
        project_plan: &ProjectPlan,
    ) -> Result<()> {
        let runtime = project_plan.primary_runtime();
        info!("Detected framework: {}", runtime.as_str());

        match runtime {
            PrimaryRuntime::Node => self.provision_nodejs(config)?,
            PrimaryRuntime::Python => self.provision_python(config)?,
            PrimaryRuntime::Ruby => self.provision_ruby(config)?,
            PrimaryRuntime::Rust => self.provision_rust()?,
            PrimaryRuntime::Go => self.provision_go(config)?,
            PrimaryRuntime::Unknown => warn!("Unknown framework, skipping"),
        }

        self.provision_databases(config)?;
        Ok(())
    }

    fn ensure_nodejs_runtime(&self, config: &VmConfig) -> Result<()> {
        let node_version = config
            .versions
            .as_ref()
            .and_then(|versions| versions.node.as_deref())
            .unwrap_or("20");
        let nvm_version = config
            .versions
            .as_ref()
            .and_then(|versions| versions.nvm.as_deref())
            .unwrap_or("v0.40.3");

        let install_script = format!(
            r#"
            export NVM_DIR="$HOME/.nvm"
            if [ ! -s "$NVM_DIR/nvm.sh" ]; then
                curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/{}/install.sh | bash
            fi
            [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
            nvm install {}
            nvm alias default {}
            nvm use {}
        "#,
            nvm_version, node_version, node_version, node_version
        );

        self.ssh_exec(&install_script)?;
        Ok(())
    }

    /// Provisions Node.js using nvm.
    /// Note: This uses `curl | bash` for nvm installation, which is a trade-off for convenience
    /// over a more secure, but complex, installation method.
    fn provision_nodejs(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Node.js dependencies");
        self.ensure_nodejs_runtime(config)?;
        self.ssh_exec(&format!(
            r#"export PATH="{}"
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
if [ -f {}/package.json ]; then cd {} && npm install; fi"#,
            Self::user_bin_path(config),
            self.project_dir,
            self.project_dir
        ))?;
        Ok(())
    }

    /// Provisions Python using pyenv.
    /// Note: This uses `curl | bash` for pyenv installation, which is a trade-off for convenience
    /// over a more secure, but complex, installation method.
    fn provision_python(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Python dependencies");
        self.ensure_python_runtime(config)?;
        self.ensure_python_package_tooling(config)?;
        self.ssh_exec(&format!(
            r#"if [ -f {}/requirements.txt ]; then
  cd {}
  if [ ! -d .venv ]; then
    python3 -m venv .venv
  fi
  . .venv/bin/activate
  pip install -r requirements.txt
fi"#,
            self.project_dir, self.project_dir
        ))?;
        Ok(())
    }

    fn ensure_python_runtime(&self, config: &VmConfig) -> Result<()> {
        let python_version = config
            .versions
            .as_ref()
            .and_then(|versions| versions.python.as_deref())
            .unwrap_or("3.11");

        let install_script = format!(
            r#"
            if ! command -v pyenv &> /dev/null; then
                curl https://pyenv.run | bash
                export PATH="$HOME/.pyenv/bin:$PATH"
                eval "$(pyenv init -)"
            fi

            pyenv install -s {}
            pyenv global {}
        "#,
            python_version, python_version
        );

        self.ssh_exec(&install_script)?;
        Ok(())
    }

    fn ensure_python_package_tooling(&self, config: &VmConfig) -> Result<()> {
        self.ssh_exec(&format!(
            r#"if ! command -v pipx >/dev/null 2>&1; then
  {}
fi
export PATH="{}"
pipx ensurepath >/dev/null 2>&1 || true"#,
            if self.is_macos_guest(config) {
                format!("{}\nbrew install pipx", Self::macos_brew_preamble())
            } else {
                "sudo apt-get update && sudo apt-get install -y pipx python3-pip python3-venv"
                    .to_string()
            },
            Self::user_bin_path(config)
        ))?;
        Ok(())
    }

    fn provision_ruby(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Ruby dependencies");
        self.ssh_exec(&format!(
            r#"{}
if [ -f {}/Gemfile ]; then
  if ! command -v bundle >/dev/null 2>&1; then gem install bundler; fi
  cd {} && bundle install
fi"#,
            if self.is_macos_guest(config) {
                format!(
                    "{}\nif ! command -v ruby >/dev/null 2>&1; then brew install ruby; fi",
                    Self::macos_brew_preamble()
                )
            } else {
                "sudo apt-get update && sudo apt-get install -y ruby-full build-essential zlib1g-dev"
                    .to_string()
            },
            self.project_dir,
            self.project_dir
        ))?;
        Ok(())
    }

    fn provision_rust(&self) -> Result<()> {
        info!("Installing Rust dependencies");
        self.ensure_rust_runtime()?;
        self.ssh_exec(&format!(
            r#"export PATH="$HOME/.cargo/bin:$PATH"
if [ -f {}/Cargo.toml ]; then
  cd {} && cargo fetch
fi"#,
            self.project_dir, self.project_dir
        ))?;
        Ok(())
    }

    fn provision_go(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Go dependencies");
        self.ssh_exec(&format!(
            r#"if ! command -v go >/dev/null 2>&1; then
  {}
fi
if [ -f {}/go.mod ]; then
  cd {} && go mod download
fi"#,
            if self.is_macos_guest(config) {
                format!("{}\nbrew install go", Self::macos_brew_preamble())
            } else {
                "sudo apt-get update && sudo apt-get install -y golang-go".to_string()
            },
            self.project_dir,
            self.project_dir
        ))?;
        Ok(())
    }

    fn ensure_rust_runtime(&self) -> Result<()> {
        self.ssh_exec(
            r#"if [ ! -x "$HOME/.cargo/bin/cargo" ]; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi
export PATH="$HOME/.cargo/bin:$PATH"
rustup default stable"#,
        )?;
        Ok(())
    }
}
