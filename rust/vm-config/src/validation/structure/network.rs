use std::collections::HashSet;

use tracing::warn;
use vm_core::error::{Result, VmError};

use crate::config::VmConfig;

pub(super) fn validate_ports(config: &VmConfig) -> Result<()> {
    let mut used_host_ports = HashSet::new();
    for mapping in &config.ports.mappings {
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
    }

    if let Some(range) = &config.ports.range {
        if range.len() != 2 {
            return Err(vm_core::error::VmError::Config(
                "Invalid port range: must have exactly 2 elements".to_string(),
            ));
        }
        let (start, end) = (range[0], range[1]);
        if start == 0 {
            return Err(vm_core::error::VmError::Config(
                "Invalid port range: port 0 is reserved".to_string(),
            ));
        }
        crate::ports::PortRange::new(start, end)?;

        for mapping in &config.ports.mappings {
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

pub(super) fn validate_services(config: &VmConfig) -> Result<()> {
    for (name, service) in &config.services {
        if service.enabled && service.port.is_none() && name != "docker" {
            return Err(VmError::Config(format!(
                "Service '{name}' is enabled but has no port specified"
            )));
        }
        if let Some(port) = service.port {
            if port == 0 {
                return Err(vm_core::error::VmError::Config(format!(
                    "Invalid port {port} for service {name}: port 0 is reserved"
                )));
            }
        }
    }

    Ok(())
}

pub(super) fn validate_networking(config: &VmConfig) -> Result<()> {
    if let Some(networking) = &config.networking {
        for network_name in &networking.networks {
            // Docker network names must be 1-64 characters
            if network_name.is_empty() || network_name.len() > 64 {
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
                return Err(VmError::Config(format!(
                    "Invalid network name '{}': must start with alphanumeric or underscore, and contain only alphanumeric, hyphens, underscores, and periods",
                    network_name
                )));
            }
        }
    }
    Ok(())
}
