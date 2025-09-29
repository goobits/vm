use vm_core::error::{Result, VmError};

/// Checks if the system meets the minimum resource requirements.
pub fn check_system_resources() -> Result<()> {
    vm_core::system_check::check_system_resources()
        .map_err(|e| VmError::Internal(format!("System check failed: {}", e)))
}
