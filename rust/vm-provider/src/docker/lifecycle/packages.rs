//! Package management utilities for pip/pipx/npm/cargo
use super::LifecycleOperations;
use serde_json::Value;
use vm_core::error::{Result, VmError};

impl<'a> LifecycleOperations<'a> {
    /// Helper to extract pipx managed packages (for filtering from pip_packages)
    #[must_use = "package extraction results should be checked"]
    pub(super) fn extract_pipx_managed_packages(
        &self,
        pipx_json: &Value,
    ) -> std::collections::HashSet<String> {
        let mut pipx_managed_packages = std::collections::HashSet::new();

        if let Some(venvs) = pipx_json.get("venvs").and_then(|v| v.as_object()) {
            for package in &self.config.pip_packages {
                if venvs.contains_key(package) {
                    pipx_managed_packages.insert(package.clone());
                }
            }
        }

        pipx_managed_packages
    }

    /// Helper to get pipx JSON output
    #[must_use = "pipx command results should be checked"]
    pub(super) fn get_pipx_json(&self) -> Result<Option<Value>> {
        if self.config.pip_packages.is_empty() {
            return Ok(None);
        }

        let pipx_list_output = std::process::Command::new("pipx")
            .args(["list", "--json"])
            .output()?;

        if pipx_list_output.status.success() {
            let pipx_json = serde_json::from_slice::<Value>(&pipx_list_output.stdout)
                .map_err(|e| VmError::Internal(format!("Failed to parse pipx package listing output as JSON. pipx may have returned invalid output: {e}")))?;
            Ok(Some(pipx_json))
        } else {
            Ok(None)
        }
    }

    /// Helper to categorize pipx packages (only returns container packages now)
    #[must_use = "package categorization results should be checked"]
    pub(super) fn categorize_pipx_packages(&self, pipx_json: &Value) -> Vec<String> {
        let mut container_pipx_packages = Vec::new();

        if let Some(venvs) = pipx_json.get("venvs").and_then(|v| v.as_object()) {
            let pipx_managed_packages: std::collections::HashSet<String> =
                venvs.keys().cloned().collect();

            for package in &self.config.pip_packages {
                if pipx_managed_packages.contains(package) {
                    // All pipx packages are now treated as PyPI packages for container installation
                    container_pipx_packages.push(package.clone());
                }
            }
        }

        container_pipx_packages
    }
}
