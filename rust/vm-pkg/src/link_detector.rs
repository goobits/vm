use crate::links::SystemLinkDetector;
use crate::package_manager::PackageManager;
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub struct LinkDetector {
    user: String,
}

impl LinkDetector {
    pub fn new(user: String) -> Self {
        Self { user }
    }

    /// Check if a package is linked (checks both .links directories and system-wide links)
    pub fn is_linked(&self, package: &str, manager: PackageManager) -> Result<bool> {
        // First check controlled .links directories
        let controlled_linked_path = self.get_linked_path(package, manager)?;
        if controlled_linked_path.is_some() {
            return Ok(true);
        }

        // Then check system-wide links
        let system_linked = self.is_system_linked(package, manager)?;
        Ok(system_linked)
    }

    /// Check if a package has system-wide links (npm global, cargo install, pip editable)
    pub fn is_system_linked(&self, package: &str, manager: PackageManager) -> Result<bool> {
        let manager_str = manager.to_string();
        let packages = vec![package.into()];

        let detections = SystemLinkDetector::detect_for_manager(&manager_str, &packages)?;
        Ok(!detections.is_empty())
    }

    /// Get the path to a linked package if it exists
    pub fn get_linked_path(
        &self,
        package: &str,
        manager: PackageManager,
    ) -> Result<Option<PathBuf>> {
        let links_dir = manager.links_dir(&self.user);

        // Direct package name check
        let direct_path = links_dir.join(package);
        if direct_path.exists() {
            return Ok(Some(direct_path));
        }

        // For Python packages, also check sanitized name (replace - with _)
        if matches!(manager, PackageManager::Pip) {
            let safe_name = package.replace('-', "_").to_lowercase();
            let safe_path = links_dir.join(&safe_name);
            if safe_path.exists() {
                return Ok(Some(safe_path));
            }
        }

        Ok(None)
    }

    /// List all linked packages for a manager (includes both controlled links and system links)
    pub fn list_linked(
        &self,
        manager: Option<PackageManager>,
    ) -> Result<Vec<(PackageManager, String)>> {
        let mut results = Vec::new();

        let managers = match manager {
            Some(m) => vec![m],
            None => vec![
                PackageManager::Cargo,
                PackageManager::Npm,
                PackageManager::Pip,
            ],
        };

        for mgr in managers {
            // Get controlled links from .links directories
            let links_dir = mgr.links_dir(&self.user);
            if links_dir.exists() {
                if let Ok(entries) = fs::read_dir(&links_dir) {
                    for entry in entries.flatten() {
                        if let Some(name) = entry.file_name().to_str() {
                            results.push((mgr, format!("{} (controlled)", name)));
                        }
                    }
                }
            }

            // Get system-wide links (but we need to scan common packages - simplified approach)
            // For a full implementation, this would require scanning all installed packages
            // For now, we'll just indicate that system link detection is available
            results.push((
                mgr,
                "(use 'vm-pkg links detect' for system-wide detection)".into(),
            ));
        }

        results.sort_by(|a, b| a.0.to_string().cmp(&b.0.to_string()).then(a.1.cmp(&b.1)));
        Ok(results)
    }

    /// Check if a path is a pipx environment
    pub fn is_pipx_environment(path: &Path) -> bool {
        vm_detector::is_pipx_environment(path)
    }

    /// Check if a path is a Python project (has setup.py or pyproject.toml)
    pub fn is_python_project(path: &Path) -> bool {
        vm_detector::is_python_project(path)
    }
}
