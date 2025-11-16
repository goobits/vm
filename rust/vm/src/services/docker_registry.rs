//! Docker Registry Service Implementation

use anyhow::Result;
use std::time::Duration;
use tracing::warn;
use vm_config::GlobalConfig;

use super::ManagedService;

/// Docker Registry service that implements the ManagedService trait
pub struct DockerRegistryService;

impl DockerRegistryService {
    /// Create a new DockerRegistryService instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for DockerRegistryService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ManagedService for DockerRegistryService {
    async fn start(&self, global_config: &GlobalConfig) -> Result<()> {
        use vm_docker_registry::{
            auto_manager::start_auto_manager, docker_config::configure_docker_daemon,
            server::start_registry_with_config, RegistryConfig,
        };

        let port = self.get_port(global_config);

        // Create custom registry config with the specified port
        let config = RegistryConfig {
            registry_port: port,
            ..Default::default()
        };

        // Start the registry service with custom config
        tokio::spawn(async move {
            if let Err(e) = start_registry_with_config(&config).await {
                warn!("Docker registry exited with error: {}", e);
            }
        });

        // Wait a moment for the service to be available
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Configure the Docker daemon with the correct registry URL
        let registry_url = format!("http://127.0.0.1:{port}");
        if let Err(e) = configure_docker_daemon(&registry_url).await {
            warn!("Failed to auto-configure Docker daemon: {}", e);
        }

        // Start the auto-manager background task
        if let Err(e) = start_auto_manager() {
            warn!("Failed to start registry auto-manager: {}", e);
        }

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        use vm_docker_registry;
        vm_docker_registry::stop_registry().await
    }

    async fn check_health(&self, global_config: &GlobalConfig) -> bool {
        let port = self.get_port(global_config);
        let endpoint = format!("http://localhost:{port}/v2/");

        // Use reqwest to check health for HTTP-based services
        match reqwest::get(&endpoint).await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    fn name(&self) -> &str {
        "docker_registry"
    }

    fn get_port(&self, global_config: &GlobalConfig) -> u16 {
        global_config.services.docker_registry.port
    }
}
