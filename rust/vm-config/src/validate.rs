use crate::config::VmConfig;
use anyhow::Result;
use regex::Regex;
use std::path::PathBuf;

/// Configuration validator
pub struct ConfigValidator {
    config: VmConfig,
}

impl ConfigValidator {
    pub fn new(config: VmConfig, _schema_path: PathBuf) -> Self {
        Self { config }
    }

    /// Validate the entire configuration
    pub fn validate(&self) -> Result<()> {
        // Run manual validation
        self.validate_manual()?;
        Ok(())
    }

    /// Full manual validation (used as fallback)
    fn validate_manual(&self) -> Result<()> {
        self.validate_required_fields()?;
        self.validate_provider()?;
        self.validate_project()?;
        self.validate_ports()?;
        self.validate_services()?;
        self.validate_versions()?;
        Ok(())
    }

    /// Check required fields are present
    fn validate_required_fields(&self) -> Result<()> {
        if self.config.provider.is_none() {
            anyhow::bail!("Missing required field: provider");
        }

        if let Some(project) = &self.config.project {
            if project.name.is_none() {
                anyhow::bail!("Missing required field: project.name");
            }
        } else {
            anyhow::bail!("Missing required field: project");
        }

        Ok(())
    }

    /// Validate provider setting
    fn validate_provider(&self) -> Result<()> {
        if let Some(provider) = &self.config.provider {
            match provider.as_str() {
                "docker" | "vagrant" | "tart" => Ok(()),
                _ => anyhow::bail!(
                    "Invalid provider: {}. Must be one of: docker, vagrant, tart",
                    provider
                ),
            }
        } else {
            Ok(())
        }
    }

    /// Validate project configuration
    fn validate_project(&self) -> Result<()> {
        if let Some(project) = &self.config.project {
            // Validate project name (alphanumeric, dashes, underscores)
            if let Some(name) = &project.name {
                let name_regex = Regex::new(r"^[a-zA-Z0-9\-_]+$")?;
                if !name_regex.is_match(name) {
                    anyhow::bail!("Invalid project name: {}. Must contain only alphanumeric characters, dashes, and underscores", name);
                }
            }

            // Validate hostname if present
            if let Some(hostname) = &project.hostname {
                let hostname_regex = Regex::new(r"^[a-zA-Z0-9\-\.]+$")?;
                if !hostname_regex.is_match(hostname) {
                    anyhow::bail!("Invalid hostname: {}. Must be a valid hostname", hostname);
                }
            }

            // Validate workspace path
            if let Some(path) = &project.workspace_path {
                if !path.starts_with('/') {
                    anyhow::bail!("Workspace path must be absolute: {}", path);
                }
            }
        }

        Ok(())
    }

    /// Validate port mappings
    fn validate_ports(&self) -> Result<()> {
        // Validate individual port mappings
        for (name, port) in &self.config.ports {
            if *port == 0 {
                anyhow::bail!("Invalid port {} for {}: port 0 is reserved", port, name);
            }
        }

        // Validate port range if present
        if let Some(range) = &self.config.port_range {
            vm_ports::PortRange::parse(range)?;
        }

        Ok(())
    }

    /// Validate service configurations
    fn validate_services(&self) -> Result<()> {
        for (name, service) in &self.config.services {
            // Validate service port if specified
            if let Some(port) = service.port {
                if port == 0 {
                    anyhow::bail!(
                        "Invalid port {} for service {}: port 0 is reserved",
                        port,
                        name
                    );
                }
            }

            // Validate GPU type if GPU service is enabled
            if name == "gpu" && service.enabled {
                if let Some(gpu_type) = &service.r#type {
                    match gpu_type.as_str() {
                        "nvidia" | "amd" | "intel" | "auto" => {}
                        _ => anyhow::bail!(
                            "Invalid GPU type: {}. Must be one of: nvidia, amd, intel, auto",
                            gpu_type
                        ),
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate version specifications
    fn validate_versions(&self) -> Result<()> {
        if let Some(versions) = &self.config.versions {
            // Validate Node.js version
            if let Some(node) = &versions.node {
                if !Self::is_valid_version(node) {
                    anyhow::bail!("Invalid Node.js version: {}", node);
                }
            }

            // Validate Python version
            if let Some(python) = &versions.python {
                if !Self::is_valid_version(python) {
                    anyhow::bail!("Invalid Python version: {}", python);
                }
            }
        }

        Ok(())
    }

    /// Check if a version string is valid
    fn is_valid_version(version: &str) -> bool {
        // Allow "latest", semantic versions, or major version numbers
        version == "latest"
            || version == "lts"
            || version.parse::<u32>().is_ok()
            || Regex::new(r"^\d+\.\d+(\.\d+)?$")
                .expect("Semantic version regex should be valid")
                .is_match(version)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config() {
        let mut config = VmConfig::default();
        config.provider = Some("docker".to_string());
        config.project = Some(crate::config::ProjectConfig {
            name: Some("test-project".to_string()),
            hostname: Some("test.local".to_string()),
            workspace_path: Some(
                crate::paths::get_default_workspace_path()
                    .to_string_lossy()
                    .to_string(),
            ),
            backup_pattern: None,
            env_template_path: None,
        });

        let validator = ConfigValidator::new(config, std::path::PathBuf::from("test.yaml"));
        assert!(validator.validate().is_ok());
    }

    #[test]
    fn test_invalid_provider() {
        let mut config = VmConfig::default();
        config.provider = Some("invalid".to_string());
        config.project = Some(crate::config::ProjectConfig {
            name: Some("test".to_string()),
            ..Default::default()
        });

        let validator = ConfigValidator::new(config, std::path::PathBuf::from("test.yaml"));
        assert!(validator.validate().is_err());
    }

    #[test]
    fn test_invalid_port() {
        let mut config = VmConfig::default();
        config.provider = Some("docker".to_string());
        config.project = Some(crate::config::ProjectConfig {
            name: Some("test".to_string()),
            ..Default::default()
        });
        config.ports.insert("web".to_string(), 0); // Port 0 is invalid

        let validator = ConfigValidator::new(config, std::path::PathBuf::from("test.yaml"));
        assert!(validator.validate().is_err());
    }
}
