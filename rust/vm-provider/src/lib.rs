use anyhow::Result;
use std::path::Path;
use vm_config::config::VmConfig;

pub mod error;
pub mod progress;
pub mod utils;

mod docker;
mod vagrant;
mod tart;

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
}

/// A factory function to create a provider instance based on the configuration.
pub fn get_provider(config: VmConfig) -> Result<Box<dyn Provider>> {
    let provider_name = config.provider.as_deref().unwrap_or("docker");
    match provider_name {
        "docker" => Ok(Box::new(docker::DockerProvider::new(config)?)),
        "vagrant" => Ok(Box::new(vagrant::VagrantProvider::new(config)?)),
        "tart" => Ok(Box::new(tart::TartProvider::new(config)?)),
        _ => Err(error::ProviderError::UnknownProvider(provider_name.to_string()).into()),
    }
}
