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

    /// Parse `tart list` output into InstanceInfo
    pub fn parse_tart_list(&self) -> Result<Vec<InstanceInfo>> {
        let output = std::process::Command::new("tart")
            .args(["list"])
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

        let list_output = String::from_utf8_lossy(&output.stdout);
        let mut instances = Vec::new();
        let project_prefix = format!("{}-", self.project_name());

        // Parse Tart list output format:
        // NAME         STATUS     ARCH     CPU     MEMORY
        // myproject-dev running   arm64    2       4GB
        for line in list_output.lines().skip(1) {
            // Skip header line
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[0];
                let status = parts[1];

                // Only include VMs that belong to this project
                if name.starts_with(&project_prefix) || name == self.project_name() {
                    // Try to get additional metadata for this VM
                    let (created_at, uptime) = self.get_vm_metadata(name);
                    instances.push(create_tart_instance_info(
                        name,
                        status,
                        created_at.as_deref(),
                        uptime.as_deref(),
                    ));
                }
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

        assert_eq!(manager.default_instance_name(), "testproject-dev");
    }

    #[test]
    fn test_resolve_instance_name_default() {
        let config = create_test_config();
        let manager = TartInstanceManager::new(&config);

        let result = manager.resolve_instance_name(None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "testproject-dev");
    }

    // Note: Tests requiring actual Tart command execution are not included
    // as they would require Tart to be installed in the test environment
}
