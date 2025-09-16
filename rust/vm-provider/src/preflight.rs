use anyhow::Result;

/// Checks if the system meets the minimum resource requirements.
pub fn check_system_resources() -> Result<()> {
    crate::system_check::check_system_resources()
}
