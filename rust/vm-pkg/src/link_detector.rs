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

    /// Check if a package is linked
    pub fn is_linked(&self, package: &str, manager: PackageManager) -> Result<bool> {
        let linked_path = self.get_linked_path(package, manager)?;
        Ok(linked_path.is_some())
    }

    /// Get the path to a linked package if it exists
    pub fn get_linked_path(&self, package: &str, manager: PackageManager) -> Result<Option<PathBuf>> {
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

    /// List all linked packages for a manager
    pub fn list_linked(&self, manager: Option<PackageManager>) -> Result<Vec<(PackageManager, String)>> {
        let mut results = Vec::new();

        let managers = match manager {
            Some(m) => vec![m],
            None => vec![PackageManager::Cargo, PackageManager::Npm, PackageManager::Pip],
        };

        for mgr in managers {
            let links_dir = mgr.links_dir(&self.user);
            if links_dir.exists() {
                if let Ok(entries) = fs::read_dir(&links_dir) {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            if let Some(name) = entry.file_name().to_str() {
                                results.push((mgr, name.to_string()));
                            }
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| a.0.to_string().cmp(&b.0.to_string()).then(a.1.cmp(&b.1)));
        Ok(results)
    }

    /// Check if a path is a pipx environment
    pub fn is_pipx_environment(path: &Path) -> bool {
        path.join("pipx_metadata.json").exists()
    }

    /// Check if a path is a Python project (has setup.py or pyproject.toml)
    pub fn is_python_project(path: &Path) -> bool {
        path.join("setup.py").exists() || path.join("pyproject.toml").exists()
    }
}