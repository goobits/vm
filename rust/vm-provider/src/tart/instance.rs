//! Tart-specific instance management
//!
//! This module provides instance resolution and management for Tart VMs,
//! enabling multi-instance support through native Tart commands.

use crate::common::instance::{
    create_tart_instance_info, extract_project_name, fuzzy_match_instances, InstanceInfo,
    InstanceResolver,
};
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

/// Tart instance manager
pub struct TartInstanceManager<'a> {
    config: &'a VmConfig,
}

impl<'a> TartInstanceManager<'a> {
    pub fn new(config: &'a VmConfig) -> Self {
        Self { config }
    }

    /// Get the project name from config
    fn project_name(&self) -> &str {
        extract_project_name(self.config)
    }

    /// Parse `tart list --format json` output into InstanceInfo
    pub fn parse_tart_list(&self) -> Result<Vec<InstanceInfo>> {
        let mut command = std::process::Command::new("tart");
        if let Some(storage_path) = self
            .config
            .tart
            .as_ref()
            .and_then(|tart| tart.storage_path.as_deref())
            .filter(|path| !path.trim().is_empty())
        {
            command.env("TART_HOME", expand_tilde(storage_path));
        }

        let output = command
            .args(["list", "--format", "json"])
            .output()
            .map_err(|e| {
                VmError::Internal(format!(
                    "Failed to execute 'tart list'. Ensure Tart is installed and accessible: {}",
                    e
                ))
            })?;

        if !output.status.success() {
            return Err(VmError::Internal(
                "Tart list command failed. Check that Tart is properly installed and configured"
                    .to_string(),
            ));
        }

        #[derive(serde::Deserialize)]
        struct TartListEntry {
            #[serde(rename = "Name")]
            name: String,
            #[serde(rename = "State")]
            state: String,
            #[serde(rename = "Source")]
            source: String,
        }

        let entries: Vec<TartListEntry> = serde_json::from_slice(&output.stdout).map_err(|e| {
            VmError::Internal(format!("Failed to parse Tart list JSON output: {e}"))
        })?;
        let mut instances = Vec::new();
        let project_prefix = format!("{}-", self.project_name());

        for entry in entries {
            if entry.source != "local" {
                continue;
            }

            let name = entry.name;
            let status = entry.state;

            // Only include VMs that belong to this project
            if name.starts_with(&project_prefix) || name == self.project_name() {
                let (created_at, uptime) = self.get_vm_metadata(&name);
                instances.push(create_tart_instance_info(
                    &name,
                    &status,
                    created_at.as_deref(),
                    uptime.as_deref(),
                ));
            }
        }

        Ok(instances)
    }

    /// Generate project-specific instance name: {project-name}-{instance}
    fn project_instance_name(&self, instance: &str) -> String {
        format!("{}-{}", self.project_name(), instance)
    }

    /// Find VM by partial name matching
    pub fn find_matching_vm(&self, partial: &str) -> Result<String> {
        let instances = self.parse_tart_list()?;
        fuzzy_match_instances(partial, &instances)
    }

    /// Get metadata for a Tart VM (created_at, uptime)
    /// Returns (None, None) as Tart doesn't easily expose this info
    fn get_vm_metadata(&self, _vm_name: &str) -> (Option<String>, Option<String>) {
        // Tart's `list` command doesn't include creation time or uptime
        // Could potentially parse ~/.tart/vms/{name} directory metadata,
        // but that's fragile and undocumented.
        // Return None for both to keep implementation simple.
        (None, None)
    }
}

fn expand_tilde(path: &str) -> String {
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}

impl<'a> InstanceResolver for TartInstanceManager<'a> {
    fn resolve_instance_name(&self, partial: Option<&str>) -> Result<String> {
        match partial {
            Some(name) => {
                // First try exact match with project prefix
                let candidate_name = self.project_instance_name(name);
                let instances = self.parse_tart_list()?;

                // Check if exact project-instance name exists
                if instances.iter().any(|i| i.name == candidate_name) {
                    return Ok(candidate_name);
                }

                // Fall back to fuzzy matching
                self.find_matching_vm(name)
            }
            None => {
                // For Tart, default to just project name (no -dev suffix like Docker)
                Ok(self.project_name().to_string())
            }
        }
    }

    fn list_instances(&self) -> Result<Vec<InstanceInfo>> {
        self.parse_tart_list()
    }

    fn default_instance_name(&self) -> String {
        // Tart uses project name directly, no -dev suffix
        self.project_name().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::{ProjectConfig, VmConfig};

    fn create_test_config() -> VmConfig {
        VmConfig {
            project: Some(ProjectConfig {
                name: Some("testproject".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_project_instance_name() {
        let config = create_test_config();
        let manager = TartInstanceManager::new(&config);

        assert_eq!(manager.project_instance_name("dev"), "testproject-dev");
        assert_eq!(
            manager.project_instance_name("staging"),
            "testproject-staging"
        );
    }

    #[test]
    fn test_project_name_fallback() {
        let config = VmConfig::default(); // No project name
        let manager = TartInstanceManager::new(&config);

        assert_eq!(manager.project_name(), "vm-project");
        assert_eq!(manager.project_instance_name("dev"), "vm-project-dev");
    }

    #[test]
    fn test_default_instance_name() {
        let config = create_test_config();
        let manager = TartInstanceManager::new(&config);

        assert_eq!(manager.default_instance_name(), "testproject");
    }

    #[test]
    fn test_resolve_instance_name_default() {
        let config = create_test_config();
        let manager = TartInstanceManager::new(&config);

        let result = manager.resolve_instance_name(None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "testproject");
    }

    // Note: Tests requiring actual Tart command execution are not included
    // as they would require Tart to be installed in the test environment
}
