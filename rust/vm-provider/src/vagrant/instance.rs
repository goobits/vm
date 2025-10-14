//! Vagrant-specific instance management
//!
//! This module provides instance resolution and management for Vagrant VMs,
//! enabling multi-machine support through Vagrant's native multi-machine capabilities.

use crate::common::instance::{
    create_vagrant_instance_info, extract_project_name, fuzzy_match_instances, InstanceInfo,
    InstanceResolver,
};
use std::path::PathBuf;
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

/// Vagrant instance manager
pub struct VagrantInstanceManager<'a> {
    config: &'a VmConfig,
    project_dir: &'a PathBuf,
}

impl<'a> VagrantInstanceManager<'a> {
    pub fn new(config: &'a VmConfig, project_dir: &'a PathBuf) -> Self {
        Self {
            config,
            project_dir,
        }
    }

    /// Get the project name from config
    fn project_name(&self) -> &str {
        extract_project_name(self.config)
    }

    /// Parse `vagrant status` for current project
    pub fn parse_vagrant_status(&self) -> Result<Vec<InstanceInfo>> {
        let vagrant_cwd = self.project_dir.join("providers/vagrant");

        let output = std::process::Command::new("vagrant")
            .env("VAGRANT_CWD", &vagrant_cwd)
            .args(["status"])
            .output()
            .map_err(|e| {
                VmError::Internal(format!(
                    "Failed to execute 'vagrant status'. Ensure Vagrant is installed and accessible: {}",
                    e
                ))
            })?;

        if !output.status.success() {
            return Err(VmError::Internal(
                "Vagrant status command failed. Check that Vagrant is properly installed and configured"
                    .to_string(),
            ));
        }

        let status_output = String::from_utf8_lossy(&output.stdout);
        let mut instances = Vec::new();
        let project_name = self.project_name();

        // Parse Vagrant status output format:
        // Current machine states:
        //
        // default                   running (virtualbox)
        // web                       not created (virtualbox)
        let mut in_machine_section = false;
        for line in status_output.lines() {
            if line.contains("Current machine states:") {
                in_machine_section = true;
                continue;
            }

            if in_machine_section && line.trim().is_empty() {
                // Skip empty lines in machine section
                continue;
            }

            if in_machine_section && line.starts_with("This environment represents") {
                // End of machine section
                break;
            }

            if in_machine_section {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let machine_name = parts[0];
                    let status = parts[1];

                    // Try to get additional metadata for this machine
                    let (created_at, uptime) = self.get_machine_metadata(machine_name);
                    instances.push(create_vagrant_instance_info(
                        machine_name,
                        status,
                        project_name,
                        created_at.as_deref(),
                        uptime.as_deref(),
                    ));
                }
            }
        }

        Ok(instances)
    }

    /// Generate multi-machine Vagrantfile
    pub fn create_multi_machine_config(&self, machines: &[&str]) -> Result<()> {
        let vagrant_dir = self.project_dir.join("providers/vagrant");
        let vagrantfile_path = vagrant_dir.join("Vagrantfile");

        // Create basic multi-machine Vagrantfile template
        let vagrantfile_content = self.generate_multi_machine_vagrantfile(machines)?;

        std::fs::create_dir_all(&vagrant_dir).map_err(|e| {
            VmError::Internal(format!(
                "Failed to create Vagrant directory at {}: {}",
                vagrant_dir.display(),
                e
            ))
        })?;

        std::fs::write(&vagrantfile_path, vagrantfile_content).map_err(|e| {
            VmError::Internal(format!(
                "Failed to write Vagrantfile to {}: {}",
                vagrantfile_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    /// Generate Vagrantfile content for multiple machines
    fn generate_multi_machine_vagrantfile(&self, machines: &[&str]) -> Result<String> {
        let mut content = String::new();
        content.push_str("# -*- mode: ruby -*-\n");
        content.push_str("# vi: set ft=ruby :\n\n");
        content.push_str("Vagrant.configure(\"2\") do |config|\n");

        // Add VM configuration from vm.yaml if available
        self.add_vm_provider_config(&mut content)?;

        // Generate machine definitions
        for machine in machines {
            content.push_str(&format!(
                "  config.vm.define \"{}\" do |{}|\n",
                machine, machine
            ));
            content.push_str(&format!("    {}.vm.box = \"ubuntu/focal64\"\n", machine));
            content.push_str(&format!(
                "    {}.vm.hostname = \"{}-{}\"\n",
                machine,
                self.project_name(),
                machine
            ));

            content.push_str("  end\n\n");
        }

        content.push_str("end\n");
        Ok(content)
    }

    /// Find machine by partial name matching
    pub fn find_matching_machine(&self, partial: &str) -> Result<String> {
        let instances = self.parse_vagrant_status()?;
        fuzzy_match_instances(partial, &instances)
    }

    /// Add VM provider configuration to content
    fn add_vm_provider_config(&self, content: &mut String) -> Result<()> {
        let vm_config = match &self.config.vm {
            Some(config) => config,
            None => return Ok(()),
        };

        let memory = match &vm_config.memory {
            Some(mem) => mem,
            None => return Ok(()),
        };

        let mb = match memory.to_mb() {
            Some(mb) => mb,
            None => return Ok(()),
        };

        content.push_str("  config.vm.provider \"virtualbox\" do |vb|\n");
        content.push_str(&format!("    vb.memory = \"{}\"\n", mb));

        if let Some(cpus) = vm_config.cpus {
            content.push_str(&format!("    vb.cpus = {}\n", cpus));
        }

        content.push_str("  end\n\n");
        Ok(())
    }

    /// Get metadata for a Vagrant machine (created_at, uptime)
    /// Returns (None, None) as Vagrant doesn't easily expose this info
    fn get_machine_metadata(&self, _machine_name: &str) -> (Option<String>, Option<String>) {
        // Vagrant doesn't provide easy access to creation time or uptime
        // via `vagrant status`. Would need to parse VirtualBox/VMware
        // provider-specific commands which isn't portable.
        // Return None for both to avoid complexity.
        (None, None)
    }
}

impl<'a> InstanceResolver for VagrantInstanceManager<'a> {
    fn resolve_instance_name(&self, partial: Option<&str>) -> Result<String> {
        match partial {
            Some(name) => self.find_matching_machine(name),
            None => Ok("default".to_string()), // Vagrant's default machine name
        }
    }

    fn list_instances(&self) -> Result<Vec<InstanceInfo>> {
        self.parse_vagrant_status()
    }

    fn default_instance_name(&self) -> String {
        "default".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
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
    fn test_project_name() {
        let config = create_test_config();
        let project_dir = PathBuf::from("/test");
        let manager = VagrantInstanceManager::new(&config, &project_dir);

        assert_eq!(manager.project_name(), "testproject");
    }

    #[test]
    fn test_project_name_fallback() {
        let config = VmConfig::default(); // No project name
        let project_dir = PathBuf::from("/test");
        let manager = VagrantInstanceManager::new(&config, &project_dir);

        assert_eq!(manager.project_name(), "vm-project");
    }

    #[test]
    fn test_default_instance_name() {
        let config = create_test_config();
        let project_dir = PathBuf::from("/test");
        let manager = VagrantInstanceManager::new(&config, &project_dir);

        assert_eq!(manager.default_instance_name(), "default");
    }

    #[test]
    fn test_generate_multi_machine_vagrantfile() {
        let config = create_test_config();
        let project_dir = PathBuf::from("/test");
        let manager = VagrantInstanceManager::new(&config, &project_dir);

        let machines = vec!["web", "db"];
        let content = manager
            .generate_multi_machine_vagrantfile(&machines)
            .expect("Should be able to generate Vagrantfile content");

        assert!(content.contains("config.vm.define \"web\""));
        assert!(content.contains("config.vm.define \"db\""));
        assert!(content.contains("testproject-web"));
        assert!(content.contains("testproject-db"));
    }

    // Note: Tests requiring actual Vagrant command execution are not included
    // as they would require Vagrant to be installed in the test environment
}
