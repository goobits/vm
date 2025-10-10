use crate::config::VmConfig;
use regex::Regex;
use std::collections::HashSet;
use std::net::TcpListener;
use std::path::PathBuf;
use tracing::warn;
use vm_core::error::{Result, VmError};
use vm_core::vm_error;

/// Validate GPU type for GPU service
fn validate_gpu_type(gpu_type: &Option<String>) -> Result<()> {
    if let Some(gpu_type) = gpu_type {
        if !matches!(gpu_type.as_str(), "nvidia" | "amd" | "intel" | "auto") {
            return Err(vm_core::error::VmError::Config(format!(
                "Invalid GPU type: {}",
                gpu_type
            )));
        }
    }
    Ok(())
}

/// Checks if a given host port is available to bind to.
fn check_port_available(port: u16, binding: &str) -> Result<()> {
    let addr = format!("{}:{}", binding, port);
    match TcpListener::bind(&addr) {
        Ok(_) => Ok(()), // Listener is implicitly closed when it goes out of scope
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                return Err(VmError::Config(format!(
                    "Port {} is already in use on host",
                    port
                )));
            }
            Err(e.into())
        }
    }
}

pub struct ConfigValidator {
    config: VmConfig,
    skip_port_availability_check: bool,
}

impl ConfigValidator {
    pub fn new(
        config: VmConfig,
        _schema_path: PathBuf,
        skip_port_availability_check: bool,
    ) -> Self {
        Self {
            config,
            skip_port_availability_check,
        }
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_manual()?;
        Ok(())
    }

    fn validate_manual(&self) -> Result<()> {
        self.validate_required_fields()?;
        self.validate_provider()?;
        self.validate_project()?;
        self.validate_ports()?;
        self.validate_services()?;
        self.validate_versions()?;
        Ok(())
    }

    fn validate_required_fields(&self) -> Result<()> {
        if self.config.provider.is_none() {
            return Err(vm_core::error::VmError::Config(
                "Missing required field: provider".to_string(),
            ));
        }

        if let Some(project) = &self.config.project {
            if project.name.is_none() {
                return Err(vm_core::error::VmError::Config(
                    "Missing required field: project.name".to_string(),
                ));
            }
        } else {
            return Err(vm_core::error::VmError::Config(
                "Missing required field: project".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_provider(&self) -> Result<()> {
        if let Some(provider) = &self.config.provider {
            match provider.as_str() {
                "docker" | "vagrant" | "tart" => Ok(()),
                _ => Err(vm_core::error::VmError::Config(format!(
                    "Invalid provider: {}",
                    provider
                ))),
            }
        } else {
            Ok(())
        }
    }

    fn validate_project(&self) -> Result<()> {
        if let Some(project) = &self.config.project {
            if let Some(name) = &project.name {
                let name_regex = Regex::new(r"^[a-zA-Z0-9\-_]+$")
                    .map_err(|e| VmError::Config(format!("Invalid regex pattern: {}", e)))?;
                if !name_regex.is_match(name) {
                    vm_error!(
                        "Invalid project name: {}. Must contain only alphanumeric characters, dashes, and underscores",
                        name
                    );
                    return Err(vm_core::error::VmError::Config(
                        "Invalid project name".to_string(),
                    ));
                }
            }

            if let Some(hostname) = &project.hostname {
                let hostname_regex = Regex::new(r"^[a-zA-Z0-9\-\.]+$")
                    .map_err(|e| VmError::Config(format!("Invalid regex pattern: {}", e)))?;
                if !hostname_regex.is_match(hostname) {
                    vm_error!("Invalid hostname: {}. Must be a valid hostname", hostname);
                    return Err(vm_core::error::VmError::Config(
                        "Invalid hostname".to_string(),
                    ));
                }
            }

            if let Some(path) = &project.workspace_path {
                if !path.starts_with('/') {
                    vm_error!("Workspace path must be absolute: {}", path);
                    return Err(vm_core::error::VmError::Config(
                        "Workspace path must be absolute".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    fn validate_ports(&self) -> Result<()> {
        let mut used_host_ports = HashSet::new();
        let port_binding = self
            .config
            .vm
            .as_ref()
            .and_then(|v| v.port_binding.as_deref())
            .unwrap_or("0.0.0.0");

        for mapping in &self.config.ports.mappings {
            if !used_host_ports.insert(mapping.host) {
                return Err(VmError::Config(format!(
                    "Duplicate host port mapping: {}",
                    mapping.host
                )));
            }

            if mapping.host == 0 || mapping.guest == 0 {
                return Err(VmError::Config(
                    "Port numbers must be greater than 0".to_string(),
                ));
            }

            if mapping.host < 1024 {
                warn!(
                    "Host port {} may require root/admin privileges",
                    mapping.host
                );
            }

            // Only check for port availability if not skipped
            if !self.skip_port_availability_check {
                check_port_available(mapping.host, port_binding)?;
            }
        }

        if let Some(range) = &self.config.ports.range {
            if range.len() != 2 {
                vm_error!("Invalid port range: must have exactly 2 elements [start, end]");
                return Err(vm_core::error::VmError::Config(
                    "Invalid port range: must have exactly 2 elements".to_string(),
                ));
            }
            let (start, end) = (range[0], range[1]);
            if start >= end {
                vm_error!(
                    "Invalid port range: start ({}) must be less than end ({})",
                    start,
                    end
                );
                return Err(vm_core::error::VmError::Config(
                    "Invalid port range".to_string(),
                ));
            }
            if start == 0 {
                vm_error!("Invalid port range: port 0 is reserved");
                return Err(vm_core::error::VmError::Config(
                    "Port 0 is reserved".to_string(),
                ));
            }

            for mapping in &self.config.ports.mappings {
                if mapping.guest >= start && mapping.guest <= end {
                    warn!(
                        "Guest port {} from explicit mapping conflicts with auto-allocated range {}-{}",
                        mapping.guest, start, end
                    );
                }
            }
        }

        Ok(())
    }

    fn validate_services(&self) -> Result<()> {
        for (name, service) in &self.config.services {
            if let Some(port) = service.port {
                if port == 0 {
                    vm_error!(
                        "Invalid port {} for service {}: port 0 is reserved",
                        port,
                        name
                    );
                    return Err(vm_core::error::VmError::Config(
                        "Invalid port: port 0 is reserved".to_string(),
                    ));
                }
            }

            if name == "gpu" && service.enabled {
                validate_gpu_type(&service.r#type)?;
            }
        }

        Ok(())
    }

    fn validate_versions(&self) -> Result<()> {
        if let Some(versions) = &self.config.versions {
            if let Some(node) = &versions.node {
                if !Self::is_valid_version(node) {
                    vm_error!("Invalid Node.js version: {}", node);
                    return Err(vm_core::error::VmError::Config(
                        "Invalid Node.js version".to_string(),
                    ));
                }
            }

            if let Some(python) = &versions.python {
                if !Self::is_valid_version(python) {
                    vm_error!("Invalid Python version: {}", python);
                    return Err(vm_core::error::VmError::Config(
                        "Invalid Python version".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    fn is_valid_version(version: &str) -> bool {
        version == "latest"
            || version == "lts"
            || version.parse::<u32>().is_ok()
            || Regex::new(r"^\d+\.\d+(\.\d+)?$")
                .map(|regex| regex.is_match(version))
                .unwrap_or(false)
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

        let validator = ConfigValidator::new(config, std::path::PathBuf::from("test.yaml"), false);
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

        let validator = ConfigValidator::new(config, std::path::PathBuf::from("test.yaml"), false);
        assert!(validator.validate().is_err());
    }

    #[test]
    fn test_invalid_port_range() {
        let mut config = VmConfig::default();
        config.provider = Some("docker".to_string());
        config.project = Some(crate::config::ProjectConfig {
            name: Some("test".to_string()),
            ..Default::default()
        });
        config.ports.range = Some(vec![0, 10]); // Port 0 is invalid

        let validator = ConfigValidator::new(config, std::path::PathBuf::from("test.yaml"), false);
        assert!(validator.validate().is_err());
    }
}
