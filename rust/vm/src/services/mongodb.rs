//! MongoDB Service Implementation

use anyhow::Result;
use vm_config::GlobalConfig;

use super::container::{
    loopback_healthy, loopback_port, reuse_managed_container, stop_managed_container,
    ManagedContainerSpec,
};
use super::{container_runtime, get_or_generate_password, ManagedService};

const CONTAINER_NAME: &str = "vm-mongodb-global";
const DISPLAY_NAME: &str = "MongoDB";
const IMAGE: &str = "mongo";
const GUEST_PORT: u16 = 27017;

/// MongoDB database service that implements the ManagedService trait
pub struct MongodbService;

impl MongodbService {
    /// Create a new MongodbService instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for MongodbService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ManagedService for MongodbService {
    async fn start(&self, global_config: &GlobalConfig) -> Result<()> {
        let settings = &global_config.services.mongodb;
        let executable = container_runtime(global_config);
        let spec = ManagedContainerSpec {
            name: CONTAINER_NAME,
            display_name: DISPLAY_NAME,
            image: IMAGE,
            version: &settings.version,
            host_port: settings.port,
            guest_port: GUEST_PORT,
        };

        let data_dir = shellexpand::tilde(&settings.data_dir).to_string();
        tokio::fs::create_dir_all(&data_dir).await?;

        let password = get_or_generate_password("mongodb").await?;
        if reuse_managed_container(executable, spec).await? {
            return Ok(());
        }

        let mut cmd = tokio::process::Command::new(executable);
        cmd.arg("run")
            .arg("-d")
            .arg("--name")
            .arg(CONTAINER_NAME)
            .args(["--label", "com.vm.managed=true"])
            .args(["--label", "com.vm.service=mongodb"])
            .arg("-p")
            .arg(loopback_port(settings.port, GUEST_PORT))
            .arg("-v")
            .arg(format!("{data_dir}:/data/db"))
            .arg("-e")
            .arg("MONGO_INITDB_ROOT_USERNAME=root")
            .arg("-e")
            .arg(format!("MONGO_INITDB_ROOT_PASSWORD={}", password))
            .arg(format!("mongo:{}", settings.version));

        let status = cmd.status().await?;
        if !status.success() {
            return Err(anyhow::anyhow!("Failed to start MongoDB container"));
        }

        Ok(())
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
        "mongodb"
    }

    fn get_port(&self, global_config: &GlobalConfig) -> u16 {
        global_config.services.mongodb.port
    }
}
