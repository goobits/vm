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

    /// Detect linked packages across all package managers
    pub fn detect_all(packages: &[String]) -> Result<Vec<(String, String, String)>> {
        let mut all_results = Vec::new();
        let managers = ["npm", "pip", "cargo"];

        for manager in &managers {
            if let Ok(results) = detect_packages(manager, packages) {
                for (package, path) in results {
                    all_results.push((package, path, manager.to_string()));
                }
            }
        }

        Ok(all_results)
    }

    /// Generate Docker mount strings for a specific package manager
    pub fn generate_mounts_for_manager(
        package_manager: &str,
        packages: &[String],
    ) -> Result<Vec<String>> {
        validate_package_manager(package_manager)?;
        generate_mounts(package_manager, packages)
    }

    /// Generate Docker mount strings for all package managers
    pub fn generate_all_mounts(packages: &[String]) -> Result<Vec<(String, String)>> {
        let mut all_mounts = Vec::new();
        let managers = ["npm", "pip", "cargo"];

        for manager in &managers {
            if let Ok(mounts) = generate_mounts(manager, packages) {
                for mount in mounts {
                    all_mounts.push((mount, manager.to_string()));
                }
            }
        }

        Ok(all_mounts)
    }

    /// Check if any packages are linked across all managers
    pub fn has_any_links(packages: &[String]) -> bool {
        let managers = ["npm", "pip", "cargo"];

        for manager in &managers {
            if let Ok(results) = detect_packages(manager, packages) {
                if !results.is_empty() {
                    return true;
                }
            }
        }

        false
    }
}