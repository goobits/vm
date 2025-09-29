pub mod cargo;
pub mod npm;
pub mod pip;
pub mod system;

pub use system::SystemLinkDetector;

use vm_core::error::{Result, VmError};
use vm_core::vm_error;

/// Validate package manager type
pub fn validate_package_manager(pm: &str) -> Result<()> {
    match pm {
        "npm" | "pip" | "cargo" => Ok(()),
        _ => {
            vm_error!(
                "Package manager '{}' not in whitelist: [npm, pip, cargo]",
                pm
            );
            Err(VmError::Internal(
                "Package manager not in whitelist".to_string(),
            ))
        }
    }
}

/// Detect packages for a specific package manager
pub fn detect_packages(
    package_manager: &str,
    packages: &[String],
) -> Result<Vec<(String, String)>> {
    match package_manager {
        "npm" => Ok(npm::detect_npm_packages(packages)),
        "pip" => Ok(pip::detect_pip_packages(packages)),
        "cargo" => cargo::detect_cargo_packages(packages),
        _ => {
            vm_error!(
                "Package manager '{}' not supported. Use npm, pip, or cargo.",
                package_manager
            );
            Err(VmError::Internal(
                "Package manager not supported".to_string(),
            ))
        }
    }
}
