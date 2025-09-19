//! Temporary VM operation error handling

use crate::{vm_error, vm_error_hint};

/// Handle provider not supporting temporary VMs
pub fn provider_unsupported() -> anyhow::Error {
    vm_error!("Provider does not support temporary VMs");
    vm_error_hint!("Use Docker provider for temporary VM functionality");
    anyhow::anyhow!("Provider does not support temporary VMs")
}

/// Handle no temporary VM found
pub fn temp_vm_not_found() -> anyhow::Error {
    vm_error!("No temp VM found");
    vm_error_hint!("Create one first with: vm temp create <mounts>");
    anyhow::anyhow!("No temp VM found")
}

/// Handle temp VM already exists error
pub fn temp_vm_already_exists(name: &str) -> anyhow::Error {
    vm_error!("Temp VM '{}' already exists", name);
    vm_error_hint!("Use 'vm temp destroy' to remove it first, or 'vm temp ssh' to connect");
    anyhow::anyhow!("Temp VM already exists")
}

/// Handle mount parsing failure
pub fn mount_parse_failed(mount_str: &str, reason: &str) -> anyhow::Error {
    vm_error!("Failed to parse mount '{}': {}", mount_str, reason);
    vm_error_hint!("Use format: /local/path:/container/path or /local/path");
    anyhow::anyhow!("Mount parsing failed")
}

/// Handle temp VM state corruption
pub fn state_corrupted() -> anyhow::Error {
    vm_error!("Temp VM state is corrupted");
    vm_error_hint!("Remove state file and recreate: vm temp destroy --force");
    anyhow::anyhow!("Temp VM state corrupted")
}

/// Handle operation cancelled by user
pub fn operation_cancelled() -> anyhow::Error {
    vm_error!("Operation cancelled by user");
    anyhow::anyhow!("Operation cancelled")
}

/// Handle temp VM not running
pub fn temp_vm_not_running(name: &str) -> anyhow::Error {
    vm_error!("Temp VM '{}' is not running", name);
    vm_error_hint!("Start it first with: vm temp start");
    anyhow::anyhow!("Temp VM not running")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_unsupported() {
        let err = provider_unsupported();
        assert!(err
            .to_string()
            .contains("Provider does not support temporary VMs"));
    }

    #[test]
    fn test_temp_vm_not_found() {
        let err = temp_vm_not_found();
        assert!(err.to_string().contains("No temp VM found"));
    }

    #[test]
    fn test_temp_vm_already_exists() {
        let err = temp_vm_already_exists("test-vm");
        assert!(err.to_string().contains("Temp VM already exists"));
    }
}
