use anyhow::Result;

/// Checks if the system meets the minimum resource requirements.
pub fn check_system_resources() -> Result<()> {
    vm_common::system_check::check_system_resources()
}
