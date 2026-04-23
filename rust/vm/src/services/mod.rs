//! Service trait and implementations for managed services
//!
//! This module provides a trait-based architecture for managing services.
//! Each service implements the ManagedService trait, which defines common
//! operations like start, stop, and health checking.

use anyhow::Result;
use vm_config::GlobalConfig;

pub mod auth_proxy;
pub mod docker_registry;
pub mod mongodb;
pub mod mysql;
pub mod package_registry;
pub mod postgresql;
pub mod redis;

/// Trait for managed services
#[async_trait::async_trait]
pub trait ManagedService: Send + Sync {
    /// Start the service
    async fn start(&self, global_config: &GlobalConfig) -> Result<()>;

    /// Stop the service
    async fn stop(&self) -> Result<()>;

    /// Check if the service is healthy
    async fn check_health(&self, global_config: &GlobalConfig) -> bool;

    /// Get the service name
    fn name(&self) -> &str;

    /// Get the port for this service
    fn get_port(&self, global_config: &GlobalConfig) -> u16;
}

/// Get a password from the secrets store, or generate a new one if it doesn't exist.
pub async fn get_or_generate_password(service_name: &str) -> Result<String> {
    vm_core::secrets::get_or_generate_password(service_name)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get password: {}", e))
}

pub fn container_runtime(global_config: &GlobalConfig) -> &str {
    global_config
        .defaults
        .provider
        .as_deref()
        .filter(|provider| matches!(*provider, "docker" | "podman"))
        .unwrap_or("docker")
}

pub fn default_container_runtime() -> String {
    vm_config::AppConfig::load(None, None, None)
        .ok()
        .and_then(|config| {
            config
                .vm
                .provider
                .or(config.global.defaults.provider)
                .filter(|provider| matches!(provider.as_str(), "docker" | "podman"))
        })
        .unwrap_or_else(|| "docker".to_string())
}
