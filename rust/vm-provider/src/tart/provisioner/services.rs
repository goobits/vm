use super::TartProvisioner;
use tracing::info;
use vm_config::config::VmConfig;
use vm_core::error::Result;

impl TartProvisioner {
    /// Provisions selected databases using the guest's native package tooling.
    pub(super) fn provision_databases(&self, config: &VmConfig) -> Result<()> {
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

    pub(super) fn run_custom_provision_scripts(&self, _config: &VmConfig) -> Result<()> {
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
    pub(super) fn start_services(&self, _config: &VmConfig) -> Result<()> {
        info!("Starting configured services");
        // Services are started by systemctl in their respective install functions.
        Ok(())
    }
}
