pub mod cargo;
pub mod npm;
pub mod pip;
pub mod system;

pub use system::SystemLinkDetector;

use anyhow::Result;
use vm_common::vm_error;

/// Validate package manager type
pub fn validate_package_manager(pm: &str) -> Result<()> {
    match pm {
        "npm" | "pip" | "cargo" => Ok(()),
        _ => {
            vm_error!(
                "Package manager '{}' not in whitelist: [npm, pip, cargo]",
                pm
            );
            return Err(anyhow::anyhow!("Package manager not in whitelist"));
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
            return Err(anyhow::anyhow!("Package manager not supported"));
        }
    }
}
