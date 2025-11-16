//! PostgreSQL Service Implementation

use anyhow::Result;
use tracing::warn;
use vm_config::GlobalConfig;

use super::{get_or_generate_password, ManagedService};

/// PostgreSQL database service that implements the ManagedService trait
pub struct PostgresqlService;

impl PostgresqlService {
    /// Create a new PostgresqlService instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for PostgresqlService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ManagedService for PostgresqlService {
    async fn start(&self, global_config: &GlobalConfig) -> Result<()> {
        let settings = &global_config.services.postgresql;
        let container_name = "vm-postgres-global";

        // Expand tilde in data_dir
        let data_dir = shellexpand::tilde(&settings.data_dir).to_string();
        tokio::fs::create_dir_all(&data_dir).await?;

        let password = get_or_generate_password("postgresql").await?;

        let mut cmd = tokio::process::Command::new("docker");
        cmd.arg("run")
            .arg("-d")
            .arg("--name")
            .arg(container_name)
            .arg("-p")
            .arg(format!("{}:5432", settings.port))
            .arg("-v")
            .arg(format!("{data_dir}:/var/lib/postgresql/data"))
            .arg("-e")
            .arg(format!("POSTGRES_PASSWORD={}", password))
            .arg(format!("postgres:{}", settings.version));

        let status = cmd.status().await?;
        if !status.success() {
            return Err(anyhow::anyhow!("Failed to start PostgreSQL container"));
        }

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let container_name = "vm-postgres-global";

        // Stop the container
        let mut stop_cmd = tokio::process::Command::new("docker");
        stop_cmd.arg("stop").arg(container_name);
        if !stop_cmd.status().await?.success() {
            warn!("Failed to stop PostgreSQL container, it may not have been running.");
        }

        // Remove the container
        let mut rm_cmd = tokio::process::Command::new("docker");
        rm_cmd.arg("rm").arg(container_name);
        if !rm_cmd.status().await?.success() {
            warn!("Failed to remove PostgreSQL container.");
        }

        Ok(())
    }

    async fn check_health(&self, global_config: &GlobalConfig) -> bool {
        let port = self.get_port(global_config);
        // For database services, a TCP connection is a reliable health check
        tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .is_ok()
    }

    fn name(&self) -> &str {
        "postgresql"
    }

    fn get_port(&self, global_config: &GlobalConfig) -> u16 {
        global_config.services.postgresql.port
    }
}
