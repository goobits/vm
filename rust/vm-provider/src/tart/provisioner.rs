use duct::cmd;
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

        // 3. Detect framework and install dependencies
        self.provision_framework_dependencies(config)?;

        // 4. Run custom provision scripts if present
        self.run_custom_provision_scripts(config)?;

        // 5. Start services
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

    fn ensure_workspace_mount(&self) -> Result<()> {
        let mount_cmd = format!(
            "if ! mount | grep -q \"on {dir} \"; then \
            if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=\"\"; fi; \
            $SUDO mkdir -p {dir} && $SUDO mount -t virtiofs workspace {dir}; \
            fi",
            dir = self.project_dir
        );

        self.ssh_exec(&mount_cmd).map(|_| ())
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

    /// Provisions Node.js using nvm.
    /// Note: This uses `curl | bash` for nvm installation, which is a trade-off for convenience
    /// over a more secure, but complex, installation method.
    fn provision_nodejs(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Node.js dependencies");
        let node_version = config
            .versions
            .as_ref()
            .and_then(|v| v.node.as_deref())
            .unwrap_or("20");

        let install_script = format!(
            r#"
            if ! command -v nvm &> /dev/null; then
                curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
                export NVM_DIR="$HOME/.nvm"
                [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
            fi

            nvm install {}
            nvm use {}

            if [ -f {}/package.json ]; then
                cd {} && npm install
            fi
        "#,
            node_version, node_version, self.project_dir, self.project_dir
        );

        self.ssh_exec(&install_script)?;
        Ok(())
    }

    /// Provisions Python using pyenv.
    /// Note: This uses `curl | bash` for pyenv installation, which is a trade-off for convenience
    /// over a more secure, but complex, installation method.
    fn provision_python(&self, config: &VmConfig) -> Result<()> {
        info!("Installing Python dependencies");
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

            if [ -f {}/requirements.txt ]; then
                cd {} && pip install -r requirements.txt
            fi
        "#,
            python_version, python_version, self.project_dir, self.project_dir
        );

        self.ssh_exec(&install_script)?;
        Ok(())
    }

    fn provision_ruby(&self, _config: &VmConfig) -> Result<()> {
        warn!("Ruby provisioning for Tart is not yet implemented.");
        Ok(())
    }

    fn provision_rust(&self, _config: &VmConfig) -> Result<()> {
        warn!("Rust provisioning for Tart is not yet implemented.");
        Ok(())
    }

    fn provision_go(&self, _config: &VmConfig) -> Result<()> {
        warn!("Go provisioning for Tart is not yet implemented.");
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
