use std::collections::HashSet;

use anyhow::Result;

use crate::types::{Plugin, ServiceContent};

use super::{ValidationError, ValidationResult};

pub(super) fn validate_port_conflicts(
    plugin: &Plugin,
    result: &mut ValidationResult,
) -> Result<()> {
    let content = match crate::discovery::load_service_content(plugin) {
        Ok(c) => c,
        Err(_) => return Ok(()), // Already reported in basic validation
    };

    // Get all installed plugins
    let plugins = match crate::discovery::discover_plugins() {
        Ok(p) => p,
        Err(_) => return Ok(()), // Can't check conflicts if discovery fails
    };

    let service_plugins = crate::discovery::get_service_plugins(&plugins);

    // Extract ports from this plugin
    let mut this_ports = HashSet::new();
    for port_mapping in &content.ports {
        if let Some(host_port) = extract_host_port(port_mapping) {
            this_ports.insert(host_port);
        }
    }

    // Check against other plugins
    for other_plugin in service_plugins {
        // Skip self
        if other_plugin.info.name == plugin.info.name {
            continue;
        }

        check_plugin_port_conflict(other_plugin, &this_ports, result);
    }

    Ok(())
}

/// Check for port conflicts with a specific plugin
fn check_plugin_port_conflict(
    other_plugin: &Plugin,
    this_ports: &HashSet<u16>,
    result: &mut ValidationResult,
) {
    let Ok(other_content) = crate::discovery::load_service_content(other_plugin) else {
        return;
    };

    for port_mapping in &other_content.ports {
        let Some(host_port) = extract_host_port(port_mapping) else {
            continue;
        };

        if this_ports.contains(&host_port) {
            result.add_error(
                ValidationError::new(
                    "ports",
                    format!(
                        "Port {} conflicts with existing plugin '{}'",
                        host_port, other_plugin.info.name
                    ),
                )
                .with_suggestion(format!(
                    "Change the host port to an unused port (e.g., {})",
                    find_available_port(host_port, this_ports)
                )),
            );
        }
    }
}

/// Extract host port from port mapping string
fn extract_host_port(port_mapping: &str) -> Option<u16> {
    let parts: Vec<&str> = port_mapping.split(':').collect();

    match parts.len() {
        1 => parts[0].parse::<u16>().ok(),
        2 => parts[0].parse::<u16>().ok(),
        _ => None,
    }
}

/// Find an available port near the requested port
fn find_available_port(base_port: u16, used_ports: &HashSet<u16>) -> u16 {
    for offset in 1..100 {
        let candidate = base_port.saturating_add(offset);
        if !used_ports.contains(&candidate) && candidate < 65535 {
            return candidate;
        }
    }
    base_port.saturating_add(100)
}

pub(super) fn validate(plugin: &Plugin, result: &mut ValidationResult) -> Result<()> {
    let content = match crate::discovery::load_service_content(plugin) {
        Ok(c) => c,
        Err(e) => {
            result.add_error(
                ValidationError::new(
                    "service_content",
                    format!("Failed to parse service.yaml: {e}"),
                )
                .with_suggestion("Check YAML syntax and structure"),
            );
            return Ok(());
        }
    };

    validate_service_image(&content, result);
    validate_service_ports(&content, result);
    validate_service_volumes(&content, result);
    super::validate_environment(&content.environment, result);

    Ok(())
}

fn validate_service_image(content: &ServiceContent, result: &mut ValidationResult) {
    if content.image.is_empty() {
        result.add_error(
            ValidationError::new("image", "Docker image cannot be empty")
                .with_suggestion("Specify a Docker image like 'postgres:15' or 'redis:7-alpine'"),
        );
        return;
    }

    // Check for image format (registry/image:tag or image:tag)
    if !content.image.contains(':') {
        result.add_warning(
            "Docker image does not specify a tag. Consider using a specific version tag."
                .to_string(),
        );
    }

    // Warn about 'latest' tag
    if content.image.ends_with(":latest") {
        result.add_warning(
            "Using 'latest' tag is not recommended. Pin to a specific version for reproducibility."
                .to_string(),
        );
    }
}

/// Validate service ports
fn validate_service_ports(content: &ServiceContent, result: &mut ValidationResult) {
    for port in &content.ports {
        validate_port_mapping(port, result);
    }
}

/// Validate service volumes
fn validate_service_volumes(content: &ServiceContent, result: &mut ValidationResult) {
    for volume in &content.volumes {
        // Check volume format (source:target or named_volume:target)
        if !volume.contains(':') {
            result.add_error(
                ValidationError::new("volumes", format!("Invalid volume format: '{volume}'"))
                    .with_suggestion("Use format 'source:target' or 'volume_name:target'"),
            );
        }
    }
}

/// Validate service environment variables
/// Validate port mapping format
pub(super) fn validate_port_mapping(port: &str, result: &mut ValidationResult) {
    let parts: Vec<&str> = port.split(':').collect();

    match parts.len() {
        1 => {
            // Container port only
            if let Err(e) = parts[0].parse::<u16>() {
                result.add_error(
                    ValidationError::new(
                        "ports",
                        format!("Invalid port number: '{}' - {}", parts[0], e),
                    )
                    .with_suggestion("Use a valid port number (1-65535)"),
                );
            }
        }
        2 => {
            // Host:container port mapping
            if let Err(e) = parts[0].parse::<u16>() {
                result.add_error(
                    ValidationError::new(
                        "ports",
                        format!("Invalid host port: '{}' - {}", parts[0], e),
                    )
                    .with_suggestion("Use a valid port number (1-65535)"),
                );
            }
            if let Err(e) = parts[1].parse::<u16>() {
                result.add_error(
                    ValidationError::new(
                        "ports",
                        format!("Invalid container port: '{}' - {}", parts[1], e),
                    )
                    .with_suggestion("Use a valid port number (1-65535)"),
                );
            }
        }
        _ => {
            result.add_error(
                ValidationError::new("ports", format!("Invalid port mapping format: '{port}'"))
                    .with_suggestion("Use format 'port' or 'host_port:container_port'"),
            );
        }
    }
}
