use crate::links::*;
use anyhow::Result;

/// System-wide package link detector that coordinates detection across all package managers
pub struct SystemLinkDetector;

impl SystemLinkDetector {
    /// Detect linked packages for a specific package manager
    pub fn detect_for_manager(
        package_manager: &str,
        packages: &[String],
    ) -> Result<Vec<(String, String)>> {
        validate_package_manager(package_manager)?;
        detect_packages(package_manager, packages)
    }
}
