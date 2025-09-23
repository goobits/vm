use crate::config::VmConfig;
use anyhow::Result;
use regex::Regex;
use std::path::PathBuf;
use vm_common::{errors, vm_error};

/// Validate GPU type for GPU service
fn validate_gpu_type(gpu_type: &Option<String>) -> Result<()> {
    if let Some(gpu_type) = gpu_type {
        if !matches!(gpu_type.as_str(), "nvidia" | "amd" | "intel" | "auto") {
            return Err(errors::validation::invalid_gpu_type(gpu_type));
        }
    }
    Ok(())
}

/// VM configuration validator.
///
/// Provides comprehensive validation of VM configurations to ensure they are
/// complete, valid, and suitable for VM provisioning. The validator checks
/// required fields, validates constraints, and ensures configuration consistency.
///
/// ## Validation Categories
/// - **Required Fields**: Essential fields that must be present
/// - **Provider Validation**: Provider-specific configuration checks
/// - **Project Settings**: Project configuration validation
/// - **Port Configuration**: Port allocation and conflict checking
/// - **Service Configuration**: Service-specific validation rules
/// - **Version Constraints**: Version format and compatibility checking
///
/// # Examples
/// ```rust
/// use vm_config::validate::ConfigValidator;
/// use vm_config::config::VmConfig;
/// use std::path::PathBuf;
///
/// let config = VmConfig::default();
/// let validator = ConfigValidator::new(config, PathBuf::from("schema.json"));
///
/// match validator.validate() {
///     Ok(()) => println!("Configuration is valid"),
///     Err(e) => println!("Validation error: {}", e),
/// }
/// ```
pub struct ConfigValidator {
    config: VmConfig,
}

impl ConfigValidator {
    /// Create a new configuration validator.
    ///
    /// # Arguments
    /// * `config` - The VM configuration to validate
    /// * `_schema_path` - Path to validation schema (currently unused, reserved for future use)
    ///
    /// # Returns
    /// A new `ConfigValidator` instance ready to perform validation
    pub fn new(config: VmConfig, _schema_path: PathBuf) -> Self {
        Self { config }
    }

    /// Validate the entire configuration.
    ///
    /// Performs comprehensive validation of all configuration aspects including
    /// required fields, provider settings, project configuration, ports, services,
    /// and version specifications.
    ///
    /// # Returns
    /// `Ok(())` if all validation checks pass
    ///
    /// # Errors
    /// Returns an error with detailed information about the first validation
    /// failure encountered. Common validation errors include:
    /// - Missing required fields
    /// - Invalid provider configuration
    /// - Port conflicts or invalid port numbers
    /// - Invalid service configurations
    /// - Malformed version specifications
    ///
    /// # Examples
    /// ```rust
    /// use vm_config::validate::ConfigValidator;
    /// use vm_config::config::VmConfig;
    /// use std::path::PathBuf;
    ///
    /// let config = VmConfig::default();
    /// let validator = ConfigValidator::new(config, PathBuf::from("schema.json"));
    ///
    /// if let Err(e) = validator.validate() {
    ///     eprintln!("Validation failed: {}", e);
    /// }
    /// ```
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
            return Err(errors::validation::missing_required_field("provider"));
        }

        if let Some(project) = &self.config.project {
            if project.name.is_none() {
                return Err(errors::validation::missing_required_field("project.name"));
            }
        } else {
            return Err(errors::validation::missing_required_field("project"));
        }

        Ok(())
    }

    /// Validate provider setting
    fn validate_provider(&self) -> Result<()> {
        if let Some(provider) = &self.config.provider {
            match provider.as_str() {
                "docker" | "vagrant" | "tart" => Ok(()),
                _ => Err(errors::validation::invalid_provider(provider)),
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
                    vm_error!(
                        "Invalid project name: {}. Must contain only alphanumeric characters, dashes, and underscores",
                        name
                    );
                    return Err(anyhow::anyhow!("Invalid project name"));
                }
            }

            // Validate hostname if present
            if let Some(hostname) = &project.hostname {
                let hostname_regex = Regex::new(r"^[a-zA-Z0-9\-\.]+$")?;
                if !hostname_regex.is_match(hostname) {
                    vm_error!("Invalid hostname: {}. Must be a valid hostname", hostname);
                    return Err(anyhow::anyhow!("Invalid hostname"));
                }
            }

            // Validate workspace path
            if let Some(path) = &project.workspace_path {
                if !path.starts_with('/') {
                    vm_error!("Workspace path must be absolute: {}", path);
                    return Err(anyhow::anyhow!("Workspace path must be absolute"));
                }
            }
        }

        Ok(())
    }

    /// Validate port mappings
    fn validate_ports(&self) -> Result<()> {
        // Validate manual ports
        for (name, &port) in &self.config.ports.manual_ports {
            if port == 0 {
                vm_error!("Invalid port {} for {}: port 0 is reserved", port, name);
                return Err(anyhow::anyhow!("Invalid port: port 0 is reserved"));
            }
        }

        // Validate _range if present
        if let Some(range) = &self.config.ports.range {
            if range.len() != 2 {
                vm_error!("Invalid port range: must have exactly 2 elements [start, end]");
                return Err(anyhow::anyhow!(
                    "Invalid port range: must have exactly 2 elements"
                ));
            }
            let (start, end) = (range[0], range[1]);
            if start >= end {
                vm_error!(
                    "Invalid port range: start ({}) must be less than end ({})",
                    start,
                    end
                );
                return Err(anyhow::anyhow!("Invalid port range"));
            }
            if start == 0 {
                vm_error!("Invalid port range: port 0 is reserved");
                return Err(anyhow::anyhow!("Port 0 is reserved"));
            }
        }

        Ok(())
    }

    /// Validate service configurations
    fn validate_services(&self) -> Result<()> {
        for (name, service) in &self.config.services {
            // Validate service port if specified
            if let Some(port) = service.port {
                if port == 0 {
                    vm_error!(
                        "Invalid port {} for service {}: port 0 is reserved",
                        port,
                        name
                    );
                    return Err(anyhow::anyhow!("Invalid port: port 0 is reserved"));
                }
            }

            // Validate GPU type if GPU service is enabled
            if name == "gpu" && service.enabled {
                validate_gpu_type(&service.r#type)?;
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
                    vm_error!("Invalid Node.js version: {}", node);
                    return Err(anyhow::anyhow!("Invalid Node.js version"));
                }
            }

            // Validate Python version
            if let Some(python) = &versions.python {
                if !Self::is_valid_version(python) {
                    vm_error!("Invalid Python version: {}", python);
                    return Err(anyhow::anyhow!("Invalid Python version"));
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
        config.ports.manual_ports.insert("web".to_string(), 0); // Port 0 is invalid

        let validator = ConfigValidator::new(config, std::path::PathBuf::from("test.yaml"));
        assert!(validator.validate().is_err());
    }
}
