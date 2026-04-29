use super::host_sync::{
    collect_host_sync_mounts, expand_tilde, file_name, resolve_guest_home_path, resolve_home_dir,
};
use super::provider::tart_run_log_path;
use duct::cmd;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use vm_config::config::{BoxSpec, VmConfig};
use vm_core::error::{Result, VmError};

mod ai_tools;
mod shell_config;

pub struct TartProvisioner {
    instance_name: String,
    project_dir: String,
}

impl TartProvisioner {
    pub fn new(instance_name: String, project_dir: String) -> Self {
        Self {
            instance_name,
            project_dir,
        }
    }

    /// Run provisioning scripts over SSH
    pub fn provision(&self, config: &VmConfig) -> Result<()> {
        info!("Starting Tart VM provisioning for {}", self.instance_name);

        // 1. Wait for VM to be ready
        self.wait_for_ssh()?;

        // 2. Ensure workspace share is mounted
        self.ensure_workspace_mount()?;

        // 3. Apply host sync behaviors that Tart can support cleanly
        self.sync_dotfiles(config)?;
        self.sync_ssh_config(config)?;

        // 4. Apply runtime configuration that should behave the same across providers
        self.apply_git_config(config)?;
        self.apply_canonical_shell_config(config)?;
        self.apply_shell_overrides(config)?;

        // 5. Honor generic package lists from vm.yaml before framework-specific setup
        self.provision_generic_packages(config)?;
        self.provision_ai_tools(config)?;
        self.install_docker_if_requested(config)?;
        // Mount AI config after CLI installation so installers do not write into
        // host-synced config directories such as ~/.claude.
        self.ensure_host_sync_mounts(config)?;

        // 6. Detect framework and install dependencies
        self.provision_framework_dependencies(config)?;

        // 7. Run custom provision scripts if present
        self.run_custom_provision_scripts(config)?;

        // 8. Start services
        self.start_services(config)?;

        info!("Provisioning completed successfully");
        Ok(())
    }

    fn wait_for_ssh(&self) -> Result<()> {
        use std::thread;
        use std::time::Duration;

        info!("Waiting for SSH to be ready...");

        let log_path = tart_run_log_path(&self.instance_name);
        for _attempt in 1..=30 {
            let result = cmd!("tart", "exec", &self.instance_name, "echo", "ready")
                .stderr_null()
                .stdout_null()
                .run();

            if result.is_ok() {
                info!("SSH is ready");
                return Ok(());
            }

            if let Ok(log) = std::fs::read_to_string(&log_path) {
                if log.contains("The number of VMs exceeds the system limit") {
                    return Err(VmError::Provider(format!(
                        "Tart could not start because the host VM limit was reached. Stop another Tart VM and retry. Tart run log: {}{}",
                        log_path,
                        self.read_host_log_tail(&log_path, 40)
                    )));
                }
            }

            thread::sleep(Duration::from_secs(2));
        }

        let log_tail = self.read_host_log_tail(&log_path, 40);

        Err(VmError::Provider(format!(
            "SSH not ready after 60 seconds. Tart run log: {}{}",
            log_path, log_tail
        )))
    }

    fn read_host_log_tail(&self, log_path: &str, max_lines: usize) -> String {
        let Ok(content) = std::fs::read_to_string(log_path) else {
            return String::new();
        };

        let tail = content
            .lines()
            .rev()
            .take(max_lines)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");

        if tail.trim().is_empty() {
            String::new()
        } else {
            format!("\nLast {} log lines:\n{}", max_lines, tail)
        }
    }

    fn shell_escape_single_quotes(input: &str) -> String {
        input.replace('\'', "'\"'\"'")
    }

    pub(crate) fn ensure_workspace_mount(&self) -> Result<()> {
        let dir_escaped = Self::shell_escape_single_quotes(&self.project_dir);
        let mount_cmd = format!(
            r#"is_mounted() {{
  if [ -x /sbin/mount ]; then
    /sbin/mount | grep -F "on $1 " >/dev/null 2>&1
  elif command -v mount >/dev/null 2>&1; then
    mount | grep -F "on $1 " >/dev/null 2>&1
  else
    return 1
  fi
}}
dir='{dir}';
if [ -x /sbin/mount_virtiofs ]; then
  mkdir -p "$dir"
  if ! is_mounted "$dir"; then
    /sbin/mount_virtiofs workspace "$dir"
  fi
else
  if ! is_mounted "$dir"; then
    if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
    $SUDO mkdir -p "$dir" && $SUDO mount -t virtiofs workspace "$dir"
  fi
fi"#,
            dir = dir_escaped
        );

        self.ssh_exec(&mount_cmd).map(|_| ())
    }

    fn ensure_host_sync_mounts(&self, config: &VmConfig) -> Result<()> {
        let mounts = collect_host_sync_mounts(config);
        if mounts.is_empty() {
            return Ok(());
        }

        let mut commands = Vec::new();
        for mount in mounts {
            let guest_path = resolve_guest_home_path(&mount.guest_path);
            let tag = Self::shell_escape_single_quotes(&mount.tag);
            commands.push(format!(
                r#"is_mounted() {{
  if [ -x /sbin/mount ]; then
    /sbin/mount | grep -F "on $1 " >/dev/null 2>&1
  elif command -v mount >/dev/null 2>&1; then
    mount | grep -F "on $1 " >/dev/null 2>&1
  else
    return 1
  fi
}}
target="{guest_path}";
if [ -x /sbin/mount_virtiofs ]; then
  mkdir -p "$target"
  if ! is_mounted "$target"; then
    /sbin/mount_virtiofs '{tag}' "$target"
  fi
else
  if ! is_mounted "$target"; then
    if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
    $SUDO mkdir -p "$target" && $SUDO mount -t virtiofs '{tag}' "$target"
  fi
fi"#
            ));
        }

        self.ssh_exec(&commands.join("\n"))?;
        Ok(())
    }

    fn sync_dotfiles(&self, config: &VmConfig) -> Result<()> {
        let Some(host_sync) = config.host_sync.as_ref() else {
            return Ok(());
        };
        if host_sync.dotfiles.is_empty() {
            return Ok(());
        }

        let Some(home_dir) = resolve_home_dir() else {
            return Ok(());
        };

        for dotfile in &host_sync.dotfiles {
            let Some(source) = expand_tilde(dotfile) else {
                continue;
            };
            if !source.exists() {
                continue;
            }

            let guest_target = if dotfile.starts_with("~/") || dotfile == "~" {
                resolve_guest_home_path(dotfile)
            } else if source.starts_with(&home_dir) {
                let relative = source.strip_prefix(&home_dir).unwrap_or(&source);
                resolve_guest_home_path(&format!("~/{}", relative.display()))
            } else {
                continue;
            };

            self.copy_host_path_to_guest(&source, &guest_target)?;
        }

        Ok(())
    }

    fn sync_ssh_config(&self, config: &VmConfig) -> Result<()> {
        if !config
            .host_sync
            .as_ref()
            .map(|sync| sync.ssh_config)
            .unwrap_or(false)
        {
            return Ok(());
        }

        let Some(home_dir) = resolve_home_dir() else {
            return Ok(());
        };
        let ssh_config = home_dir.join(".ssh").join("config");
        if !ssh_config.exists() || !ssh_config.is_file() {
            return Ok(());
        }

        self.copy_host_path_to_guest(&ssh_config, "$HOME/.ssh/config")?;
        Ok(())
    }

    fn copy_host_path_to_guest(&self, source: &Path, guest_target: &str) -> Result<()> {
        let source = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());
        let guest_target_escaped = Self::shell_escape_single_quotes(guest_target);
        let guest_parent = Path::new(guest_target)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        let guest_parent_escaped = Self::shell_escape_single_quotes(&guest_parent);

        if source.is_file() {
            let source_escaped = Self::shell_escape_single_quotes(&source.to_string_lossy());
            let command = format!(
                "cat '{}' | tart exec {} bash -lc \"mkdir -p '{}' && cat > '{}'\"",
                source_escaped, self.instance_name, guest_parent_escaped, guest_target_escaped
            );
            cmd!("sh", "-c", command)
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to sync file to Tart VM: {}", e)))?;
            return Ok(());
        }

        if source.is_dir() {
            let Some(name) = file_name(&source) else {
                return Ok(());
            };
            let parent = source.parent().unwrap_or(source.as_path());
            let parent_escaped = Self::shell_escape_single_quotes(&parent.to_string_lossy());
            let name_escaped = Self::shell_escape_single_quotes(&name);
            let command = format!(
                "tar -C '{}' -cf - '{}' | tart exec {} bash -lc \"mkdir -p '{}' && tar -xf - -C '{}'\"",
                parent_escaped,
                name_escaped,
                self.instance_name,
                guest_parent_escaped,
                guest_parent_escaped
            );
            cmd!("sh", "-c", command).run().map_err(|e| {
                VmError::Provider(format!("Failed to sync directory to Tart VM: {}", e))
            })?;
        }

        Ok(())
    }

    fn copy_host_file_to_guest_home(
        &self,
        source: &Path,
        guest_relative_path: &str,
        mode: &str,
    ) -> Result<()> {
        if !source.exists() || !source.is_file() {
            return Ok(());
        }

        let source = source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf());
        let Some(parent) = Path::new(guest_relative_path).parent() else {
            return Ok(());
        };

        let source_escaped = Self::shell_escape_single_quotes(&source.to_string_lossy());
        let instance_escaped = Self::shell_escape_single_quotes(&self.instance_name);
        let parent_escaped = Self::shell_escape_single_quotes(&parent.to_string_lossy());
        let target_escaped = Self::shell_escape_single_quotes(guest_relative_path);
        let mode_escaped = Self::shell_escape_single_quotes(mode);
        let remote_script = format!(
            r#"set -e
mkdir -p "$HOME/{parent}"
cat > "$HOME/{target}"
home_uid="$(stat -f %u "$HOME" 2>/dev/null || stat -c %u "$HOME" 2>/dev/null || id -u)"
home_gid="$(stat -f %g "$HOME" 2>/dev/null || stat -c %g "$HOME" 2>/dev/null || id -g)"
if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
$SUDO chown "$home_uid:$home_gid" "$HOME/{target}" 2>/dev/null || true
chmod {mode} "$HOME/{target}""#,
            parent = parent_escaped,
            target = target_escaped,
            mode = mode_escaped
        );
        let remote_script_escaped = Self::shell_escape_single_quotes(&remote_script);
        let command = format!(
            "cat '{source}' | tart exec '{instance}' bash -lc '{script}'",
            source = source_escaped,
            instance = instance_escaped,
            script = remote_script_escaped
        );

        cmd!("sh", "-c", command).run().map_err(|e| {
            VmError::Provider(format!(
                "Failed to seed Tart guest file '{}': {}",
                guest_relative_path, e
            ))
        })?;

        Ok(())
    }

    fn apply_git_config(&self, config: &VmConfig) -> Result<()> {
        if !config
            .host_sync
            .as_ref()
            .map(|sync| sync.git_config)
            .unwrap_or(true)
        {
            return Ok(());
        }

        let Some(git_config) = &config.git_config else {
            return Ok(());
        };

        let mut commands = Vec::new();
        if let Some(name) = &git_config.user_name {
            commands.push(format!(
                "git config --global user.name '{}'",
                Self::shell_escape_single_quotes(name)
            ));
        }
        if let Some(email) = &git_config.user_email {
            commands.push(format!(
                "git config --global user.email '{}'",
                Self::shell_escape_single_quotes(email)
            ));
        }
        if let Some(rebase) = &git_config.pull_rebase {
            commands.push(format!(
                "git config --global pull.rebase '{}'",
                Self::shell_escape_single_quotes(rebase)
            ));
        }
        if let Some(branch) = &git_config.init_default_branch {
            commands.push(format!(
                "git config --global init.defaultBranch '{}'",
                Self::shell_escape_single_quotes(branch)
            ));
        }
        if let Some(editor) = &git_config.core_editor {
            commands.push(format!(
                "git config --global core.editor '{}'",
                Self::shell_escape_single_quotes(editor)
            ));
        }
        if let Some(content) = &git_config.core_excludesfile_content {
            commands.push(format!(
                "cat > \"$HOME/.gitignore_global\" <<'EOF'\n{}\nEOF",
                content
            ));
            commands.push(
                "git config --global core.excludesfile \"$HOME/.gitignore_global\"".to_string(),
            );
        }

        if commands.is_empty() {
            return Ok(());
        }

        self.ssh_exec(&commands.join("\n"))?;
        Ok(())
    }

    fn provision_generic_packages(&self, config: &VmConfig) -> Result<()> {
        if !config.apt_packages.is_empty() {
            let packages = config.apt_packages.join(" ");
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
            let packages = config.npm_packages.join(" ");
            self.ssh_exec(&format!("npm install -g {}", packages))?;
        }

        if !config.pip_packages.is_empty() {
            self.ensure_python_runtime(config)?;
            self.ensure_python_package_tooling(config)?;
            let packages = config.pip_packages.join(" ");
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
            let packages = config.cargo_packages.join(" ");
            self.ssh_exec(&format!(
                r#"export PATH="$HOME/.cargo/bin:$PATH"
cargo install {}"#,
                packages
            ))?;
        }

        Ok(())
    }

    fn install_docker_if_requested(&self, config: &VmConfig) -> Result<()> {
        if !config
            .tart
            .as_ref()
            .and_then(|t| t.install_docker)
            .unwrap_or(false)
        {
            return Ok(());
        }

        if self.is_macos_guest(config) {
            return Err(VmError::Config(
                "tart.install_docker is only supported for Linux Tart guests".to_string(),
            ));
        }

        self.ssh_exec(
            r#"if ! command -v docker >/dev/null 2>&1; then
  curl -fsSL https://get.docker.com | sh
  if command -v sudo >/dev/null 2>&1; then
    sudo usermod -aG docker "$USER" || true
  fi
fi"#,
        )?;

        Ok(())
    }

    fn provision_framework_dependencies(&self, config: &VmConfig) -> Result<()> {
        let framework = self.detect_framework(config)?;
        info!("Detected framework: {}", framework);

        match framework.as_str() {
            "nodejs" => self.provision_nodejs(config)?,
            "python" => self.provision_python(config)?,
            "ruby" => self.provision_ruby(config)?,
            "rust" => self.provision_rust(config)?,
            "go" => self.provision_go(config)?,
            _ => warn!("Unknown framework: {}, skipping", framework),
        }

        self.provision_databases(config)?;
        Ok(())
    }

    fn detect_framework(&self, _config: &VmConfig) -> Result<String> {
        // Framework detection is now automatic based on project files
        // VmConfig no longer has a direct framework field

        let detection_script = r#"
            if [ -f "package.json" ]; then echo "nodejs"
            elif [ -f "requirements.txt" ] || [ -f "pyproject.toml" ]; then echo "python"
            elif [ -f "Gemfile" ]; then echo "ruby"
            elif [ -f "Cargo.toml" ]; then echo "rust"
            elif [ -f "go.mod" ]; then echo "go"
            else echo "unknown"
            fi
        "#;

        let output = self.ssh_exec(&format!("cd {} && {}", self.project_dir, detection_script))?;
        Ok(output.trim().to_string())
    }

    fn ensure_nodejs_runtime(&self, config: &VmConfig) -> Result<()> {
        let node_version = config
            .versions
            .as_ref()
            .and_then(|v| v.node.as_deref())
            .unwrap_or("20");
        let nvm_version = config
            .versions
            .as_ref()
            .and_then(|v| v.nvm.as_deref())
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
            "if [ -f {}/package.json ]; then cd {} && npm install; fi",
            self.project_dir, self.project_dir
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
            .and_then(|v| v.python.as_deref())
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
            self.project_dir, self.project_dir
        ))?;
        Ok(())
    }

    fn provision_rust(&self, _config: &VmConfig) -> Result<()> {
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

    /// Provisions selected databases using the guest's native package tooling.
    fn provision_databases(&self, config: &VmConfig) -> Result<()> {
        if self.is_macos_guest(config) {
            self.ensure_homebrew()?;
        }

        // Check if postgres service is enabled
        if config
            .services
            .get("postgresql")
            .or_else(|| config.services.get("postgres"))
            .map(|s| s.enabled)
            .unwrap_or(false)
        {
            self.install_postgresql()?;
        }

        // Check if redis service is enabled
        if config
            .services
            .get("redis")
            .map(|s| s.enabled)
            .unwrap_or(false)
        {
            self.install_redis()?;
        }

        // Check if mongodb service is enabled
        if config
            .services
            .get("mongodb")
            .map(|s| s.enabled)
            .unwrap_or(false)
        {
            self.install_mongodb()?;
        }

        Ok(())
    }

    fn install_postgresql(&self) -> Result<()> {
        info!("Installing PostgreSQL");
        self.ssh_exec(
            r#"
            if [ -x /opt/homebrew/bin/brew ]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
            if command -v brew >/dev/null 2>&1; then
              brew install postgresql
              brew services start postgresql || true
            else
              sudo apt-get update
              sudo apt-get install -y postgresql postgresql-contrib
              sudo systemctl enable postgresql
              sudo systemctl start postgresql
            fi
        "#,
        )?;
        Ok(())
    }

    fn install_redis(&self) -> Result<()> {
        info!("Installing Redis");
        self.ssh_exec(
            r#"
            if [ -x /opt/homebrew/bin/brew ]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
            if command -v brew >/dev/null 2>&1; then
              brew install redis
              brew services start redis || true
            else
              sudo apt-get update
              sudo apt-get install -y redis-server
              sudo systemctl enable redis-server
              sudo systemctl start redis-server
            fi
        "#,
        )?;
        Ok(())
    }

    fn install_mongodb(&self) -> Result<()> {
        info!("Installing MongoDB");
        self.ssh_exec(
            r#"
            if [ -x /opt/homebrew/bin/brew ]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
            if command -v brew >/dev/null 2>&1; then
              brew tap mongodb/brew || true
              brew install mongodb-community || true
              brew services start mongodb-community || true
            else
              sudo apt-get update
              sudo apt-get install -y mongodb
              sudo systemctl enable mongodb
              sudo systemctl start mongodb
            fi
        "#,
        )?;
        Ok(())
    }

    fn run_custom_provision_scripts(&self, _config: &VmConfig) -> Result<()> {
        let script_path = format!("{}/provision.sh", self.project_dir);
        let check_script = format!(
            r#"
            if [ -f {} ]; then
                echo "found"
            fi
        "#,
            script_path
        );

        let output = self.ssh_exec(&check_script)?;

        if output.trim() == "found" {
            info!("Running custom provision script");
            self.ssh_exec(&format!("cd {} && bash provision.sh", self.project_dir))?;
        }

        Ok(())
    }

    /// Ensures all configured services are started.
    /// Note: This is currently a no-op because the database installation scripts
    /// (`install_postgresql`, etc.) already enable and start the services via `systemctl`.
    /// This method is kept for clarity and future use.
    fn start_services(&self, _config: &VmConfig) -> Result<()> {
        info!("Starting configured services");
        // Services are started by systemctl in their respective install functions.
        Ok(())
    }

    fn ssh_exec(&self, command: &str) -> Result<String> {
        let output = cmd!("tart", "exec", &self.instance_name, "bash", "-c", command)
            .read()
            .map_err(|e| VmError::Provider(format!("Exec command failed: {}", e)))?;

        Ok(output)
    }

    fn is_macos_guest(&self, config: &VmConfig) -> bool {
        Self::guest_os(config) == "macos"
    }

    fn guest_os(config: &VmConfig) -> &'static str {
        if matches!(config.os.as_deref(), Some("macos")) {
            return "macos";
        }

        if matches!(config.os.as_deref(), Some("linux")) {
            return "linux";
        }

        if matches!(
            config.tart.as_ref().and_then(|t| t.guest_os.as_deref()),
            Some("macos")
        ) {
            return "macos";
        }

        if matches!(
            config.tart.as_ref().and_then(|t| t.guest_os.as_deref()),
            Some("linux")
        ) {
            return "linux";
        }

        if let Some(BoxSpec::String(name)) = config.vm.as_ref().and_then(|vm| vm.get_box_spec()) {
            if name == "vibe-tart-base" || name.contains("macos") {
                return "macos";
            }
            if name == "vibe-tart-linux-base"
                || name.contains("ubuntu")
                || name.contains("debian")
                || name.contains("linux")
            {
                return "linux";
            }
        }

        if let Some(image) = config.tart.as_ref().and_then(|t| t.image.as_deref()) {
            if image.contains("macos") {
                return "macos";
            }
            if image.contains("ubuntu") || image.contains("debian") || image.contains("linux") {
                return "linux";
            }
        }

        "macos"
    }

    fn ensure_homebrew(&self) -> Result<()> {
        self.ssh_exec(
            r#"if [ -x /opt/homebrew/bin/brew ]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
fi
if ! command -v brew >/dev/null 2>&1; then
  echo "Homebrew is required for macOS Tart provisioning" >&2
  exit 1
fi"#,
        )?;
        Ok(())
    }

    fn user_bin_path(config: &VmConfig) -> &'static str {
        if Self::guest_os(config) == "macos" {
            "/opt/homebrew/bin:$HOME/.local/bin:$PATH"
        } else {
            "$HOME/.local/bin:$PATH"
        }
    }

    fn macos_brew_preamble() -> &'static str {
        r#"if [ -x /opt/homebrew/bin/brew ]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
fi"#
    }
}

#[cfg(test)]
mod tests {
    use super::TartProvisioner;
    use indexmap::IndexMap;
    use vm_config::config::{BoxSpec, ProjectConfig, TerminalConfig, VmConfig, VmSettings};

    #[test]
    fn render_shell_overrides_includes_environment_exports() {
        let mut config = VmConfig::default();
        config
            .environment
            .insert("EDITOR".to_string(), "nvim".to_string());

        let rendered = TartProvisioner::render_shell_overrides(&config).unwrap();

        assert!(rendered.contains("export EDITOR='nvim'"));
    }

    #[test]
    fn render_shell_overrides_returns_none_when_empty() {
        let config = VmConfig {
            aliases: IndexMap::new(),
            environment: IndexMap::new(),
            ..Default::default()
        };

        let rendered = TartProvisioner::render_shell_overrides(&config);
        assert!(rendered.is_none());
    }

    #[test]
    fn guest_os_detects_vibe_tart_base_as_macos() {
        let config = VmConfig {
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("vibe-tart-base".to_string())),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(TartProvisioner::guest_os(&config), "macos");
    }

    #[test]
    fn guest_os_detects_linux_base_name() {
        let config = VmConfig {
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("vibe-tart-linux-base".to_string())),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(TartProvisioner::guest_os(&config), "linux");
    }

    #[test]
    fn guest_os_respects_explicit_config_os() {
        let config = VmConfig {
            os: Some("macos".to_string()),
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("custom-base".to_string())),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(TartProvisioner::guest_os(&config), "macos");
    }

    #[test]
    fn guest_os_defaults_ambiguous_custom_tart_base_to_macos() {
        let config = VmConfig {
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("custom-team-base".to_string())),
                ..Default::default()
            }),
            tart: Some(vm_config::config::TartConfig {
                image: Some("ghcr.io/example/custom-base:latest".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(TartProvisioner::guest_os(&config), "macos");
    }

    #[test]
    fn canonical_zshrc_renders_for_macos_tart() {
        let mut config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("vm".to_string()),
                ..Default::default()
            }),
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String("vibe-tart-base".to_string())),
                ..Default::default()
            }),
            terminal: Some(TerminalConfig {
                username: Some("vm-dev".to_string()),
                theme: Some("dracula".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        config
            .aliases
            .insert("gs".to_string(), "git status".to_string());

        let rendered =
            TartProvisioner::render_canonical_zshrc(&config, "/Users/admin/workspace").unwrap();

        assert!(rendered.contains("PROMPT='🍎 "));
        assert!(rendered.contains("alias gs='git status'"));
        assert!(rendered.contains("VM_AI_ALIAS_REPAIR_VERSION=2"));
        assert!(rendered.contains("yocodex()"));
        assert!(rendered.contains("vm_repair_codex_state"));
        assert!(rendered
            .contains("VM_PROJECT_PATH=\"$(vm_b64decode 'L1VzZXJzL2FkbWluL3dvcmtzcGFjZQ==')\""));
    }
}
