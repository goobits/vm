use crate::config::{BoxSpec, VmConfig};
use std::collections::HashSet;
use std::net::TcpListener;
use std::path::PathBuf;
use tracing::warn;
use vm_core::error::{Result, VmError};
use vm_core::vm_error;

/// Validate box spec configurations are compatible with the provider
pub fn validate_box_spec(config: &VmConfig, provider: &str) -> Vec<String> {
    let mut errors = Vec::new();

    let Some(vm) = &config.vm else {
        return errors;
    };
    let Some(box_spec) = vm.get_box_spec() else {
        return errors;
    };

    match provider {
        "docker" | "podman" => validate_docker_box_spec(&box_spec, &mut errors),
        "tart" => validate_tart_box_spec(&box_spec, &mut errors),
        _ => {}
    }

    errors
}

fn validate_docker_box_spec(box_spec: &BoxSpec, errors: &mut Vec<String>) {
    if let BoxSpec::Build { dockerfile, .. } = box_spec {
        let path = std::path::Path::new(dockerfile);
        if !path.exists() {
            errors.push(format!("Dockerfile not found: {}", dockerfile));
        }
    }
}

fn validate_tart_box_spec(box_spec: &BoxSpec, errors: &mut Vec<String>) {
    if matches!(box_spec, BoxSpec::Build { .. }) {
        errors.push("Tart does not support Dockerfile builds".to_string());
    }
}

/// Checks if a given host port is available to bind to.
fn check_port_available(port: u16, binding: &str) -> Result<()> {
    let addr = format!("{binding}:{port}");
    match TcpListener::bind(&addr) {
        Ok(_) => Ok(()), // Listener is implicitly closed when it goes out of scope
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                return Err(VmError::Config(format!(
                    "Configuration error: Port {port} is already in use on host"
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
        self.validate_box_spec_compat()?;
        self.validate_project()?;
        self.validate_ports()?;
        self.validate_services()?;
        self.validate_versions()?;
        self.validate_networking()?;
        self.validate_runtime()?;
        self.validate_storage()?;
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
                "docker" | "podman" | "tart" => Ok(()),
                _ => Err(vm_core::error::VmError::Config(format!(
                    "Invalid provider: {provider}"
                ))),
            }
        } else {
            Ok(())
        }
    }

    fn validate_box_spec_compat(&self) -> Result<()> {
        if let Some(provider) = &self.config.provider {
            let errors = validate_box_spec(&self.config, provider);
            if !errors.is_empty() {
                for error in &errors {
                    vm_error!("{}", error);
                }
                return Err(vm_core::error::VmError::Config(errors.join("; ")));
            }
        }
        Ok(())
    }

    fn validate_project(&self) -> Result<()> {
        if let Some(project) = &self.config.project {
            if let Some(name) = &project.name {
                let is_valid = !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
                if !is_valid {
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
                let is_valid = !hostname.is_empty()
                    && hostname
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
                if !is_valid {
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
        if version == "latest" || version == "lts" || version.parse::<u32>().is_ok() {
            return true;
        }

        let parts: Vec<&str> = version.split('.').collect();

        if parts.len() < 2 || parts.len() > 3 {
            return false; // Must have 2-3 parts (X.Y or X.Y.Z)
        }

        for part in parts {
            if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
        }

        true
    }

    fn validate_networking(&self) -> Result<()> {
        if let Some(networking) = &self.config.networking {
            for network_name in &networking.networks {
                // Docker network names must be 1-64 characters
                if network_name.is_empty() || network_name.len() > 64 {
                    vm_error!(
                        "Invalid network name '{}': must be 1-64 characters long",
                        network_name
                    );
                    return Err(VmError::Config(format!(
                        "Invalid network name '{}': must be 1-64 characters long",
                        network_name
                    )));
                }

                // Docker network names must contain only alphanumeric, hyphens, underscores, and periods
                // and cannot start with a period or hyphen
                // Regex was: ^[a-zA-Z0-9_][a-zA-Z0-9_\-\.]*$

                let first_char_valid = network_name
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_alphanumeric() || c == '_')
                    .unwrap_or(false);

                let rest_valid = network_name
                    .chars()
                    .skip(1)
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');

                if !first_char_valid || !rest_valid {
                    vm_error!(
                        "Invalid network name '{}': must start with alphanumeric or underscore, and contain only alphanumeric, hyphens, underscores, and periods",
                        network_name
                    );
                    return Err(VmError::Config(format!(
                        "Invalid network name '{}': must start with alphanumeric or underscore, and contain only alphanumeric, hyphens, underscores, and periods",
                        network_name
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_runtime(&self) -> Result<()> {
        let Some(vm) = &self.config.vm else {
            return Ok(());
        };

        if vm.pids_limit == Some(0) {
            return Err(VmError::Config(
                "vm.pids_limit must be greater than zero".to_string(),
            ));
        }
        if vm.stop_grace_period == Some(0) {
            return Err(VmError::Config(
                "vm.stop_grace_period must be greater than zero".to_string(),
            ));
        }

        if let Some(logging) = &vm.logging {
            if !matches!(logging.driver.as_str(), "local" | "json-file") {
                return Err(VmError::Config(
                    "vm.logging.driver must be 'local' or 'json-file'".to_string(),
                ));
            }
            if !valid_size_string(&logging.max_size) {
                return Err(VmError::Config(
                    "vm.logging.max_size must be a positive size such as '20m'".to_string(),
                ));
            }
            if logging.max_files == 0 {
                return Err(VmError::Config(
                    "vm.logging.max_files must be greater than zero".to_string(),
                ));
            }
        }

        if self.config.provider.as_deref() == Some("tart")
            && (vm.pids_limit.is_some() || vm.stop_grace_period.is_some() || vm.logging.is_some())
        {
            return Err(VmError::Config(
                "Container runtime limits and logging are not supported by Tart".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_storage(&self) -> Result<()> {
        if self.config.storage.is_empty() {
            return Ok(());
        }
        if self.config.provider.as_deref() == Some("tart") {
            return Err(VmError::Config(
                "Named volumes and tmpfs mounts are not supported by Tart".to_string(),
            ));
        }

        let username = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");
        let mut targets = HashSet::from([format!("/home/{username}/.shell_history")]);
        if self
            .config
            .services
            .get("postgresql")
            .is_some_and(|service| service.enabled)
        {
            targets.insert("/var/lib/postgresql/data".to_string());
        }
        for (name, volume) in &self.config.storage.volumes {
            if !valid_storage_name(name) {
                return Err(VmError::Config(format!(
                    "Invalid storage volume name '{name}': use letters, numbers, dashes, or underscores"
                )));
            }
            if matches!(name.as_str(), "shell_history" | "postgres_data") {
                return Err(VmError::Config(format!(
                    "Storage volume name '{name}' is reserved by the VM tool"
                )));
            }
            validate_mount_target(&volume.target)?;
            if volume.target == "/workspace" {
                return Err(VmError::Config(
                    "A named volume cannot replace the /workspace source bind; use a nested target"
                        .to_string(),
                ));
            }
            if !targets.insert(volume.target.clone()) {
                return Err(VmError::Config(format!(
                    "Duplicate storage target: {}",
                    volume.target
                )));
            }
        }

        for tmpfs in &self.config.storage.tmpfs {
            validate_mount_target(&tmpfs.target)?;
            if !targets.insert(tmpfs.target.clone()) {
                return Err(VmError::Config(format!(
                    "Duplicate storage target: {}",
                    tmpfs.target
                )));
            }
            if !matches!(tmpfs.size.to_mb(), Some(size) if size > 0) {
                return Err(VmError::Config(format!(
                    "tmpfs mount '{}' requires a fixed, positive size",
                    tmpfs.target
                )));
            }
            if !(3..=4).contains(&tmpfs.mode.len())
                || !tmpfs
                    .mode
                    .chars()
                    .all(|character| matches!(character, '0'..='7'))
            {
                return Err(VmError::Config(format!(
                    "tmpfs mount '{}' has invalid mode '{}'; use three or four octal digits",
                    tmpfs.target, tmpfs.mode
                )));
            }
        }

        Ok(())
    }
}

fn valid_storage_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn validate_mount_target(target: &str) -> Result<()> {
    let path = std::path::Path::new(target);
    if !path.is_absolute()
        || target == "/"
        || target.ends_with('/')
        || target.contains("//")
        || target.contains("/../")
        || target.ends_with("/..")
        || target.contains("/./")
        || target.ends_with("/.")
    {
        return Err(VmError::Config(format!(
            "Storage target '{target}' must be a normalized absolute path below /"
        )));
    }
    Ok(())
}

fn valid_size_string(value: &str) -> bool {
    let value = value.trim();
    if value.len() < 2 {
        return false;
    }
    let (number, suffix) = value.split_at(value.len() - 1);
    number.parse::<u64>().is_ok_and(|number| number > 0)
        && matches!(suffix.to_ascii_lowercase().as_str(), "k" | "m" | "g")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        MemoryLimit, StorageConfig, TmpfsMountConfig, VolumeMountConfig, VolumeRetention,
        VolumeScope,
    };

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

    #[test]
    fn test_valid_container_storage_policy() {
        let mut config = VmConfig {
            provider: Some("docker".to_string()),
            project: Some(crate::config::ProjectConfig {
                name: Some("test".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        config.storage.volumes.insert(
            "node_modules".to_string(),
            VolumeMountConfig {
                target: "/workspace/node_modules".to_string(),
                scope: VolumeScope::Instance,
                nocopy: true,
                retention: VolumeRetention::Keep,
            },
        );
        config.storage.tmpfs.push(TmpfsMountConfig {
            target: "/tmp".to_string(),
            size: MemoryLimit::Limited(4096),
            mode: "1777".to_string(),
        });

        let validator = ConfigValidator::new(config, PathBuf::from("test.yaml"), true);
        assert!(validator.validate().is_ok());
    }

    #[test]
    fn test_storage_policy_cannot_hide_workspace_bind() {
        let mut volumes = indexmap::IndexMap::new();
        volumes.insert(
            "source".to_string(),
            VolumeMountConfig {
                target: "/workspace".to_string(),
                scope: VolumeScope::Project,
                nocopy: true,
                retention: VolumeRetention::Keep,
            },
        );
        let config = VmConfig {
            provider: Some("docker".to_string()),
            project: Some(crate::config::ProjectConfig {
                name: Some("test".to_string()),
                ..Default::default()
            }),
            storage: StorageConfig {
                volumes,
                ..Default::default()
            },
            ..Default::default()
        };

        let error = ConfigValidator::new(config, PathBuf::from("test.yaml"), true)
            .validate()
            .unwrap_err();
        assert!(error.to_string().contains("cannot replace the /workspace"));
    }

    #[test]
    fn test_storage_policy_rejects_reserved_names_and_unnormalized_targets() {
        let base = || VmConfig {
            provider: Some("docker".to_string()),
            project: Some(crate::config::ProjectConfig {
                name: Some("test".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let volume = |target: &str| VolumeMountConfig {
            target: target.to_string(),
            scope: VolumeScope::Project,
            nocopy: true,
            retention: VolumeRetention::Keep,
        };

        let mut reserved = base();
        reserved.storage.volumes.insert(
            "shell_history".to_string(),
            volume("/home/developer/history-copy"),
        );
        let error = ConfigValidator::new(reserved, PathBuf::from("test.yaml"), true)
            .validate()
            .unwrap_err();
        assert!(error.to_string().contains("reserved"));

        let mut unnormalized = base();
        unnormalized
            .storage
            .volumes
            .insert("cache".to_string(), volume("/home/developer//cache"));
        let error = ConfigValidator::new(unnormalized, PathBuf::from("test.yaml"), true)
            .validate()
            .unwrap_err();
        assert!(error.to_string().contains("normalized absolute path"));
    }
}
