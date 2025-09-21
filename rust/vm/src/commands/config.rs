// Configuration-related command handlers

use anyhow::{Context, Result};
use log::debug;
use std::path::PathBuf;

use crate::cli::ConfigSubcommand;
use serde_yaml_ng as serde_yaml;
use vm_common::{vm_error, vm_println, vm_success, vm_warning};
use vm_config::ports::{PortRange, PortRegistry};
use vm_config::{config::VmConfig, ConfigOps};

/// Handle configuration validation command
pub fn handle_validate(config_file: Option<PathBuf>) -> Result<()> {
    debug!("Validating configuration: config_file={:?}", config_file);
    // The `load` function performs validation internally. If it succeeds,
    // the configuration is valid.
    match VmConfig::load(config_file) {
        Ok(config) => {
            debug!(
                "Configuration validation successful: provider={:?}, project_name={:?}",
                config.provider,
                config.project.as_ref().and_then(|p| p.name.as_ref())
            );
            vm_success!("Configuration is valid.");
            Ok(())
        }
        Err(e) => {
            debug!("Configuration validation failed: {}", e);
            vm_error!("Configuration is invalid: {:#}", e);
            // Return the error to exit with a non-zero status code
            Err(e)
        }
    }
}

/// Handle configuration management commands
pub fn handle_config_command(command: &ConfigSubcommand, dry_run: bool) -> Result<()> {
    match command {
        ConfigSubcommand::Set {
            field,
            value,
            global,
        } => ConfigOps::set(field, value, *global, dry_run),
        ConfigSubcommand::Get { field, global } => ConfigOps::get(field.as_deref(), *global),
        ConfigSubcommand::Unset { field, global } => ConfigOps::unset(field, *global),
        ConfigSubcommand::Preset {
            names,
            global,
            list,
            show,
        } => match (list, show, names) {
            (true, _, _) => ConfigOps::preset("", *global, true, None),
            (_, Some(show_name), _) => ConfigOps::preset("", *global, false, Some(show_name)),
            (_, _, Some(preset_names)) => ConfigOps::preset(preset_names, *global, false, None),
            _ => Ok(()),
        },
        ConfigSubcommand::Ports { fix } => handle_ports_command(*fix),
    }
}

/// Load configuration with lenient validation for commands that don't require full project setup
pub fn load_config_lenient(file: Option<PathBuf>) -> Result<VmConfig> {
    use vm_config::config::VmConfig;

    // Try to load defaults as base
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../../configs/defaults.yaml");
    let mut config: VmConfig =
        serde_yaml::from_str(EMBEDDED_DEFAULTS).context("Failed to parse embedded defaults")?;

    // Try to find and load user config if it exists
    let user_config_path = match file {
        Some(path) => Some(path),
        None => {
            // Look for vm.yaml in current directory
            let current_dir = std::env::current_dir()?;
            let vm_yaml_path = current_dir.join("vm.yaml");
            if vm_yaml_path.exists() {
                Some(vm_yaml_path)
            } else {
                None
            }
        }
    };

    if let Some(path) = user_config_path {
        match VmConfig::from_file(&path) {
            Ok(user_config) => {
                // Merge user config into defaults using available public API
                // For lenient loading, we'll do a simple field-by-field merge
                if user_config.provider.is_some() {
                    config.provider = user_config.provider;
                }
                if user_config.project.is_some() {
                    config.project = user_config.project;
                }
                if user_config.vm.is_some() {
                    config.vm = user_config.vm;
                }
                // Copy other important fields
                if !user_config.services.is_empty() {
                    config.services = user_config.services;
                }
            }
            Err(e) => {
                debug!("Failed to load user config, using defaults: {}", e);
            }
        }
    }

    // Ensure we have at least a minimal valid config for providers
    if config.provider.is_none() {
        config.provider = Some(String::from("docker"));
    }

    Ok(config)
}

/// Handle ports command
pub fn handle_ports_command(fix: bool) -> Result<()> {
    debug!("Handling ports command: fix={}", fix);

    // Load current project configuration
    let config = VmConfig::load(None)?;

    // Get project name
    let project_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .context("No project name found in configuration")?;

    // Get current port range from config
    let current_port_range = config
        .port_range
        .as_ref()
        .context("No port range found in configuration")?;

    vm_println!("📡 Current port configuration:");
    vm_println!("   Project: {}", project_name);
    vm_println!("   Port range: {}", current_port_range);

    if !fix {
        // For basic ports command, just show the configuration
        return Ok(());
    }

    // Parse current range
    let current_range =
        PortRange::parse(current_port_range).context("Failed to parse current port range")?;

    // Only check for conflicts when --fix is specified
    vm_println!();
    vm_println!("🔍 Checking for port conflicts...");

    // Check for conflicts with running Docker containers
    let conflicts = check_docker_port_conflicts(&current_range)?;

    if conflicts.is_empty() {
        vm_success!("No port conflicts detected!");
        return Ok(());
    }

    vm_warning!("Port conflicts detected:");
    for conflict in &conflicts {
        vm_println!(
            "   ⚠️  Port {} is in use by: {}",
            conflict.port,
            conflict.container
        );
    }

    // Fix conflicts by finding a new port range
    vm_println!();
    vm_println!("🔧 Fixing port conflicts...");

    let registry = PortRegistry::load().context("Failed to load port registry")?;

    // Calculate range size from current range
    let range_size = current_range.size();

    // Find next available range
    let new_range_str = registry
        .suggest_next_range(range_size, 3000)
        .context("No available port ranges found")?;

    vm_println!("   📡 New port range: {}", new_range_str);

    // Update vm.yaml with new port range
    update_vm_config_ports(&new_range_str)?;

    // Update port registry
    let new_range = PortRange::parse(&new_range_str)?;
    let mut registry = PortRegistry::load()?;

    // Get current directory for registry path
    let current_dir = std::env::current_dir()?;

    registry
        .register(project_name, &new_range, &current_dir.to_string_lossy())
        .context("Failed to register new port range")?;

    vm_success!("Port conflicts resolved!");
    vm_println!("   Updated vm.yaml with new port range: {}", new_range_str);
    vm_println!("   Registered in port registry");
    vm_println!();
    vm_println!("💡 You can now run 'vm create' again");

    Ok(())
}

#[derive(Debug)]
struct PortConflict {
    port: u16,
    container: String,
}

/// Check for conflicts between the given port range and running Docker containers
fn check_docker_port_conflicts(range: &PortRange) -> Result<Vec<PortConflict>> {
    use std::process::Command;

    let mut conflicts = Vec::new();

    // Run docker ps to get running containers with port mappings
    let output = Command::new("docker")
        .args(["ps", "--format", "{{.Names}}:{{.Ports}}"])
        .output()
        .context("Failed to run docker ps command")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Docker command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let Some((container, ports)) = line.split_once(':') else {
            continue;
        };

        // Parse port mappings like "0.0.0.0:3010->3010/tcp"
        for port_mapping in ports.split(", ") {
            let Some(host_port) = extract_host_port(port_mapping) else {
                continue;
            };

            if host_port >= range.start && host_port <= range.end {
                conflicts.push(PortConflict {
                    port: host_port,
                    container: container.to_string(),
                });
            }
        }
    }

    Ok(conflicts)
}

/// Extract host port from Docker port mapping string
fn extract_host_port(port_mapping: &str) -> Option<u16> {
    // Handle formats like:
    // "0.0.0.0:3010->3010/tcp"
    // "[::]:3010->3010/tcp"
    // "3010->3010/tcp"

    if let Some(arrow_pos) = port_mapping.find("->") {
        let host_part = &port_mapping[..arrow_pos];

        // Extract port from host part
        if let Some(colon_pos) = host_part.rfind(':') {
            let port_str = &host_part[colon_pos + 1..];
            port_str.parse().ok()
        } else {
            // Direct port mapping without host
            host_part.parse().ok()
        }
    } else {
        None
    }
}

/// Update vm.yaml with new port range
fn update_vm_config_ports(new_range: &str) -> Result<()> {
    use std::fs;

    let config_path = std::env::current_dir()?.join("vm.yaml");

    if !config_path.exists() {
        return Err(anyhow::anyhow!("vm.yaml not found in current directory"));
    }

    let content = fs::read_to_string(&config_path).context("Failed to read vm.yaml")?;

    // Parse YAML
    let mut yaml: serde_yaml::Value =
        serde_yaml::from_str(&content).context("Failed to parse vm.yaml")?;

    // Update port_range field
    if let Some(mapping) = yaml.as_mapping_mut() {
        mapping.insert(
            serde_yaml::Value::String("port_range".to_string()),
            serde_yaml::Value::String(new_range.to_string()),
        );

        // Also update individual port mappings if they exist
        if let Some(ports) = mapping.get_mut(serde_yaml::Value::String("ports".to_string())) {
            if let Some(ports_map) = ports.as_mapping_mut() {
                let range = PortRange::parse(new_range)?;
                let start_port = range.start;

                // Update backend port (first port in range)
                if ports_map.contains_key(serde_yaml::Value::String("backend".to_string())) {
                    ports_map.insert(
                        serde_yaml::Value::String("backend".to_string()),
                        serde_yaml::Value::Number(serde_yaml::Number::from(start_port)),
                    );
                }

                // Update frontend port (second port in range)
                if ports_map.contains_key(serde_yaml::Value::String("frontend".to_string())) {
                    ports_map.insert(
                        serde_yaml::Value::String("frontend".to_string()),
                        serde_yaml::Value::Number(serde_yaml::Number::from(start_port + 1)),
                    );
                }
            }
        }
    }

    // Write back to file
    let updated_content =
        serde_yaml::to_string(&yaml).context("Failed to serialize updated YAML")?;

    fs::write(&config_path, updated_content).context("Failed to write updated vm.yaml")?;

    Ok(())
}
