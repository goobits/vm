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
    let secrets_dir = vm_core::user_paths::secrets_dir()?;
    tokio::fs::create_dir_all(&secrets_dir).await?;
    let secret_file = secrets_dir.join(format!("{}.env", service_name));

    if secret_file.exists() {
        let password = tokio::fs::read_to_string(secret_file).await?;
        Ok(password.trim().to_string())
    } else {
        let password = crate::utils::generate_random_password(16);
        tokio::fs::write(&secret_file, &password).await?;
        vm_core::vm_println!(
            "💡 Generated new password for {} and saved to {:?}",
            service_name,
            secret_file
        );
        Ok(password)
    }
}
