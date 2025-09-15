use anyhow::Result;
use crate::TempVmState;

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