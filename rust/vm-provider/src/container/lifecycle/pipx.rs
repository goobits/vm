use std::collections::HashSet;

use serde_json::Value;
use vm_core::error::{Result, VmError};

use super::LifecycleOperations;

impl LifecycleOperations<'_> {
    pub(super) fn extract_pipx_managed_packages(&self, pipx_json: &Value) -> HashSet<String> {
        let mut managed_packages = HashSet::new();
        if let Some(venvs) = pipx_json.get("venvs").and_then(Value::as_object) {
            for package in &self.config.pip_packages {
                if venvs.contains_key(package) {
                    managed_packages.insert(package.clone());
                }
            }
        }
        managed_packages
    }

    pub(super) fn get_pipx_json(&self) -> Result<Option<Value>> {
        if self.config.pip_packages.is_empty() {
            return Ok(None);
        }
        let output = match std::process::Command::new("pipx")
            .args(["list", "--json"])
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !output.status.success() {
            return Ok(None);
        }
        serde_json::from_slice(&output.stdout)
            .map(Some)
            .map_err(|error| {
                VmError::Internal(format!(
                    "Failed to parse pipx package listing output as JSON. pipx may have returned invalid output: {error}"
                ))
            })
    }

    pub(super) fn categorize_pipx_packages(&self, pipx_json: &Value) -> Vec<String> {
        let managed_packages = pipx_json
            .get("venvs")
            .and_then(Value::as_object)
            .map(|venvs| venvs.keys().cloned().collect::<HashSet<_>>())
            .unwrap_or_default();
        self.config
            .pip_packages
            .iter()
            .filter(|package| managed_packages.contains(*package))
            .cloned()
            .collect()
    }
}
