//! PostgreSQL Service Implementation

use anyhow::Result;
use vm_config::GlobalConfig;

use super::container::{
    loopback_healthy, start_managed_container, stop_managed_container, ManagedContainerSpec,
};
use super::{container_runtime, get_or_generate_password, ManagedService};

const CONTAINER_NAME: &str = "vm-postgres-global";
const DISPLAY_NAME: &str = "PostgreSQL";
const IMAGE: &str = "postgres";
const GUEST_PORT: u16 = 5432;

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
        let executable = container_runtime(global_config);
        let spec = ManagedContainerSpec {
            name: CONTAINER_NAME,
            service: "postgresql",
            display_name: DISPLAY_NAME,
            image: IMAGE,
            version: &settings.version,
            host_port: settings.port,
            guest_port: GUEST_PORT,
        };

        let password = get_or_generate_password("postgresql").await?;
        start_managed_container(
            executable,
            spec,
            &settings.data_dir,
            "/var/lib/postgresql/data",
            &[("POSTGRES_PASSWORD", &password)],
            &[],
        )
        .await
    }

    async fn stop(&self) -> Result<()> {
        let executable = crate::utils::configured_container_runtime();
        stop_managed_container(&executable, CONTAINER_NAME).await
    }

    async fn check_health(&self, global_config: &GlobalConfig) -> bool {
        let port = self.get_port(global_config);
        // For database services, a TCP connection is a reliable health check
        loopback_healthy(port).await
    }

    fn name(&self) -> &str {
        "postgresql"
    }

    fn get_port(&self, global_config: &GlobalConfig) -> u16 {
        global_config.services.postgresql.port
    }
}
