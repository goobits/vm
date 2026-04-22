use super::host_sync::{
    collect_host_sync_mounts, expand_tilde, file_name, resolve_guest_home_path, resolve_home_dir,
};
use duct::cmd;
use std::path::Path;
use tracing::{info, warn};
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

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
        self.ensure_host_sync_mounts(config)?;
        self.sync_dotfiles(config)?;
        self.sync_ssh_config(config)?;

        // 4. Apply runtime configuration that should behave the same across providers
        self.apply_git_config(config)?;
        self.apply_shell_overrides(config)?;

        // 5. Honor generic package lists from vm.yaml before framework-specific setup
        self.provision_generic_packages(config)?;
        self.provision_ai_tools(config)?;
        self.install_docker_if_requested(config)?;

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

        for _attempt in 1..=30 {
            let result = cmd!("tart", "exec", &self.instance_name, "echo", "ready")
                .stderr_null()
                .stdout_null()
                .run();

            if result.is_ok() {
                info!("SSH is ready");
                return Ok(());
            }

            thread::sleep(Duration::from_secs(2));
        }

        Err(VmError::Provider(
            "SSH not ready after 60 seconds".to_string(),
        ))
    }

    fn shell_escape_single_quotes(input: &str) -> String {
        input.replace('\'', "'\"'\"'")
    }

    fn ensure_workspace_mount(&self) -> Result<()> {
        let dir_escaped = Self::shell_escape_single_quotes(&self.project_dir);
        let mount_cmd = format!(
            "dir='{dir}'; \
            if ! mount | grep -F \"on $dir \"; then \
            if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=\"\"; fi; \
            $SUDO mkdir -p \"$dir\" && $SUDO mount -t virtiofs workspace \"$dir\"; \
            fi",
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
                "target=\"{guest_path}\"; if ! mount | grep -F \"on $target \"; then if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=\"\"; fi; $SUDO mkdir -p \"$target\" && $SUDO mount -t virtiofs '{tag}' \"$target\"; fi"
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

    fn apply_shell_overrides(&self, config: &VmConfig) -> Result<()> {
        let Some(overrides) = Self::render_shell_overrides(config) else {
            return Ok(());
        };

        let script = format!(
            r#"cat > "$HOME/.vm_shell_overrides" <<'EOF'
{overrides}
EOF
for shell_rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
  touch "$shell_rc"
  if ! grep -Fq '. "$HOME/.vm_shell_overrides"' "$shell_rc"; then
    printf '\n[ -f "$HOME/.vm_shell_overrides" ] && . "$HOME/.vm_shell_overrides"\n' >> "$shell_rc"
  fi
done"#
        );

        self.ssh_exec(&script)?;
        Ok(())
    }

    fn render_shell_overrides(config: &VmConfig) -> Option<String> {
        let mut lines = Vec::new();

        for (key, value) in &config.environment {
            lines.push(format!(
                "export {}='{}'",
                key,
                Self::shell_escape_single_quotes(value)
            ));
        }

        for (name, command) in &config.aliases {
            lines.push(format!(
                "alias {}='{}'",
                name,
                Self::shell_escape_single_quotes(command)
            ));
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    fn provision_generic_packages(&self, config: &VmConfig) -> Result<()> {
        if !config.apt_packages.is_empty() {
            let packages = config.apt_packages.join(" ");
            self.ssh_exec(&format!(
                "sudo apt-get update && sudo apt-get install -y {}",
                packages
            ))?;
        }

        if !config.npm_packages.is_empty() {
            self.ensure_nodejs_runtime(config)?;
            let packages = config.npm_packages.join(" ");
            self.ssh_exec(&format!("npm install -g {}", packages))?;
        }

        if !config.pip_packages.is_empty() {
            self.ensure_python_runtime(config)?;
            let packages = config.pip_packages.join(" ");
            self.ssh_exec(&format!(
                "python3 -m pip install --upgrade pip && python3 -m pip install {}",
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

    fn provision_ai_tools(&self, config: &VmConfig) -> Result<()> {
        let Some(ai_tools) = config
            .host_sync
            .as_ref()
            .and_then(|sync| sync.ai_tools.as_ref())
        else {
            return Ok(());
        };

        if ai_tools.is_claude_enabled() {
            self.ssh_exec(
                r#"if ! command -v claude >/dev/null 2>&1; then
  curl -fsSL https://claude.ai/install.sh | bash
fi"#,
            )?;
        }

        if ai_tools.is_gemini_enabled() || ai_tools.is_codex_enabled() {
            self.ensure_nodejs_runtime(config)?;
        }

        if ai_tools.is_gemini_enabled() {
            self.ssh_exec(
                "if ! command -v gemini >/dev/null 2>&1; then npm install -g @google/gemini-cli; fi",
            )?;
        }

        if ai_tools.is_codex_enabled() {
            self.ssh_exec(
                "if ! command -v codex >/dev/null 2>&1; then npm install -g @openai/codex; fi",
            )?;
        }

        if ai_tools.is_aider_enabled() {
            self.ensure_python_runtime(config)?;
            self.ssh_exec(
                "if ! command -v aider >/dev/null 2>&1; then python3 -m pip install aider-chat; fi",
            )?;
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
        self.ssh_exec(&format!(
            "if [ -f {}/requirements.txt ]; then cd {} && python3 -m pip install -r requirements.txt; fi",
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

    fn provision_ruby(&self, _config: &VmConfig) -> Result<()> {
        info!("Installing Ruby dependencies");
        self.ssh_exec(&format!(
            r#"sudo apt-get update && sudo apt-get install -y ruby-full build-essential zlib1g-dev
if [ -f {}/Gemfile ]; then
  if ! command -v bundle >/dev/null 2>&1; then gem install bundler; fi
  cd {} && bundle install
fi"#,
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

    fn provision_go(&self, _config: &VmConfig) -> Result<()> {
        info!("Installing Go dependencies");
        self.ssh_exec(&format!(
            r#"if ! command -v go >/dev/null 2>&1; then
  sudo apt-get update && sudo apt-get install -y golang-go
fi
if [ -f {}/go.mod ]; then
  cd {} && go mod download
fi"#,
            self.project_dir, self.project_dir
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

    /// Provisions selected databases.
    /// Note: This assumes a Debian-based guest OS (like Ubuntu) because it uses `apt-get`.
    /// This is a reasonable default as the default Tart image is Ubuntu-based.
    fn provision_databases(&self, config: &VmConfig) -> Result<()> {
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
            sudo apt-get update
            sudo apt-get install -y postgresql postgresql-contrib
            sudo systemctl enable postgresql
            sudo systemctl start postgresql
        "#,
        )?;
        Ok(())
    }

    fn install_redis(&self) -> Result<()> {
        info!("Installing Redis");
        self.ssh_exec(
            r#"
            sudo apt-get update
            sudo apt-get install -y redis-server
            sudo systemctl enable redis-server
            sudo systemctl start redis-server
        "#,
        )?;
        Ok(())
    }

    fn install_mongodb(&self) -> Result<()> {
        info!("Installing MongoDB");
        self.ssh_exec(
            r#"
            sudo apt-get update
            sudo apt-get install -y mongodb
            sudo systemctl enable mongodb
            sudo systemctl start mongodb
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
}

#[cfg(test)]
mod tests {
    use super::TartProvisioner;
    use indexmap::IndexMap;
    use vm_config::config::VmConfig;

    #[test]
    fn render_shell_overrides_includes_exports_and_aliases() {
        let mut config = VmConfig::default();
        config
            .environment
            .insert("EDITOR".to_string(), "nvim".to_string());
        config
            .aliases
            .insert("gs".to_string(), "git status".to_string());

        let rendered = TartProvisioner::render_shell_overrides(&config).unwrap();

        assert!(rendered.contains("export EDITOR='nvim'"));
        assert!(rendered.contains("alias gs='git status'"));
    }

    #[test]
    fn render_shell_overrides_returns_none_when_empty() {
        let config = VmConfig {
            aliases: IndexMap::new(),
            environment: IndexMap::new(),
            ..Default::default()
        };

        assert!(TartProvisioner::render_shell_overrides(&config).is_none());
    }
}
