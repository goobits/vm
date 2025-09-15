//! VM provider abstraction library.
//!
//! This library provides a unified interface for working with different VM providers
//! such as Docker, Vagrant, and Tart. It defines core traits and factory functions
//! for provider instantiation and management.

use anyhow::Result;
use std::path::Path;
use vm_config::config::VmConfig;

pub mod error;
pub mod progress;
pub mod resources;
pub mod security;
pub mod temp_models;
pub mod utils;

pub mod audio;
pub mod preflight;

mod docker;
mod tart;
mod vagrant;

pub use temp_models::{Mount, MountPermission, TempVmState};

/// Trait for providers that support temporary VM mount updates
pub trait TempProvider {
    /// Update the mounts of a temporary VM by recreating the container
    fn update_mounts(&self, state: &TempVmState) -> Result<()>;

    /// Recreate a container with new mount configuration
    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()>;

    /// Check if a container is healthy and ready
    fn check_container_health(&self, container_name: &str) -> Result<bool>;

    /// Check if a container is currently running
    fn is_container_running(&self, container_name: &str) -> Result<bool>;
}

/// The core trait for all VM providers.
/// This defines the contract for creating, managing, and interacting with a VM.
pub trait Provider {
    /// Get the name of the provider (e.g., "docker", "vagrant").
    fn name(&self) -> &'static str;

    /// Create a new VM instance.
    /// This is the main provisioning step.
    fn create(&self) -> Result<()>;

    /// Start an existing, stopped VM.
    fn start(&self) -> Result<()>;

    /// Stop a running VM without destroying it.
    fn stop(&self) -> Result<()>;

    /// Destroy a VM, removing all associated resources.
    fn destroy(&self) -> Result<()>;

    /// Open an interactive shell (SSH) into the VM.
    fn ssh(&self, relative_path: &Path) -> Result<()>;

    /// Execute a command inside the VM.
    fn exec(&self, cmd: &[String]) -> Result<()>;

    /// Get the logs of the VM.
    fn logs(&self) -> Result<()>;

    /// Get the status of the VM.
    fn status(&self) -> Result<()>;

    /// Restart a VM (stop then start).
    fn restart(&self) -> Result<()>;

    /// Re-run provisioning on existing VM.
    fn provision(&self) -> Result<()>;

    /// List all VMs.
    fn list(&self) -> Result<()>;

    /// Force kill VM processes.
    /// If container is provided, kill that specific container. Otherwise, kill the project's container.
    fn kill(&self, container: Option<&str>) -> Result<()>;

    /// Get workspace directory.
    fn get_sync_directory(&self) -> Result<String>;

    /// Get access to temp provider capabilities if supported
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        None
    }
}

// When in a test environment, include the mock provider.
#[cfg(test)]
mod mock;

/// Creates a provider instance based on the configuration.
///
/// # Arguments
/// * `config` - The VM configuration containing provider settings
///
/// # Returns
/// A boxed provider implementation or an error if the provider is unknown.
pub fn get_provider(config: VmConfig) -> Result<Box<dyn Provider>> {
    let provider_name = config.provider.as_deref().unwrap_or("docker");

    // In a test build, allow instantiating the mock provider.
    #[cfg(test)]
    if provider_name == "mock" {
        return Ok(Box::new(mock::MockProvider::new(config)?));
    }

    match provider_name {
        "docker" => Ok(Box::new(docker::DockerProvider::new(config)?)),
        "vagrant" => Ok(Box::new(vagrant::VagrantProvider::new(config)?)),
        "tart" => Ok(Box::new(tart::TartProvider::new(config)?)),
        _ => Err(error::ProviderError::UnknownProvider(provider_name.to_string()).into()),
    }
}
