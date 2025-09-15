pub mod cargo;
pub mod npm;
pub mod pip;
pub mod system;

pub use system::SystemLinkDetector;

use anyhow::Result;

/// Validate package manager type
pub fn validate_package_manager(pm: &str) -> Result<()> {
    match pm {
        "npm" | "pip" | "cargo" => Ok(()),
        _ => anyhow::bail!(
            "❌ Error: Package manager '{}' not in whitelist: [npm, pip, cargo]",
            pm
        ),
    }
}

/// Detect packages for a specific package manager
pub fn detect_packages(
    package_manager: &str,
    packages: &[String],
) -> Result<Vec<(String, String)>> {
    match package_manager {
        "npm" => npm::detect_npm_packages(packages),
        "pip" => pip::detect_pip_packages(packages),
        "cargo" => cargo::detect_cargo_packages(packages),
        _ => unreachable!(), // Already validated
    }
}

