// Configuration-related command handlers

use anyhow::Context;
use std::path::PathBuf;
use tracing::{debug, warn};

use crate::cli::ConfigSubcommand;
use crate::error::{VmError, VmResult};
use serde_yaml_ng as serde_yaml;
use vm_cli::msg;
use vm_config::ports::{PortRange, PortRegistry};
use vm_config::{config::VmConfig, validator::ConfigValidator, ConfigOps};
use vm_core::{vm_println, vm_success};
use vm_messages::messages::MESSAGES;

/// Handle the `vm config validate` command.
fn handle_validate_command() -> VmResult<()> {
    let config = VmConfig::load(None)?;
    let validator = ConfigValidator::new();
    let report = validator
        .validate(&config)
        .map_err(|e| VmError::validation(e.to_string(), None::<String>))?;

    if report.has_errors() {
        vm_println!("❌ Configuration validation failed:");
        vm_println!("{}", report);

        // Offer to apply suggested fixes
        if report.has_fixes() {
            vm_println!("");
            vm_println!("💡 Would you like to apply these fixes automatically?");

            use dialoguer::Confirm;
            if Confirm::new()
                .with_prompt("Apply suggested fixes?")
                .default(false)
                .interact()
                .unwrap_or(false)
            {
                vm_println!("");
                for fix in &report.suggested_fixes {
                    let values = vec![fix.value.clone()];
                    match ConfigOps::set(&fix.field, &values, false, false) {
                        Ok(_) => vm_success!("Applied: {} = {}", fix.field, fix.value),
                        Err(e) => warn!("Failed to apply fix for {}: {}", fix.field, e),
                    }
                }
                vm_println!("");
                vm_success!("Fixes applied! Run 'vm config validate' again to verify.");
                return Ok(());
            }
        }

        // Return a generic error to ensure non-zero exit code
        return Err(VmError::validation(
            "Validation found errors.".to_string(),
            None::<String>,
        ));
    }

    vm_println!("{}", report); // Print warnings and info
    vm_success!("Configuration is valid.");
    Ok(())
}

/// Handle the `vm config show` command.
fn handle_show_command(profile: Option<String>) -> VmResult<()> {
    let app_config = vm_config::AppConfig::load(None, profile)?;
    let config = app_config.vm;

    if let Some(source) = &config.source_path {
        vm_println!("Config source: {}", source.display());
    } else {
        vm_println!("Config source: (Not found, using defaults)");
    }

    let yaml_output = serde_yaml::to_string(&config)
        .map_err(|e| VmError::config(e, "Failed to serialize configuration to YAML"))?;

    vm_println!("\n---\n{}", yaml_output);
    Ok(())
}

/// Handle configuration management commands
pub fn handle_config_command(
    command: &ConfigSubcommand,
    dry_run: bool,
    profile: Option<String>,
) -> VmResult<()> {
    match command {
        ConfigSubcommand::Validate => handle_validate_command(),
        ConfigSubcommand::Show => handle_show_command(profile),
        ConfigSubcommand::Set {
            field,
            values,
            global,
        } => Ok(ConfigOps::set(field, values, *global, dry_run)?),
        ConfigSubcommand::Get { field, global } => Ok(ConfigOps::get(field.as_deref(), *global)?),
        ConfigSubcommand::Unset { field, global } => Ok(ConfigOps::unset(field, *global)?),
        ConfigSubcommand::Preset {
            names,
            global,
            list,
            show,
        } => match (list, show, names) {
            (true, _, _) => Ok(ConfigOps::preset("", *global, true, None)?),
            (_, Some(show_name), _) => Ok(ConfigOps::preset("", *global, false, Some(show_name))?),
            (_, _, Some(preset_names)) => {
                Ok(ConfigOps::preset(preset_names, *global, false, None)?)
            }
            _ => Ok(()),
        },
        ConfigSubcommand::Ports { fix } => handle_ports_command(*fix),
        ConfigSubcommand::Clear { global } => Ok(ConfigOps::clear(*global)?),
    }
}

/// Load configuration with lenient validation for commands that don't require full project setup
pub fn load_config_lenient(file: Option<PathBuf>) -> VmResult<VmConfig> {
    use vm_config::config::VmConfig;

    // Try to load defaults as base
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../../configs/defaults.yaml");
    let mut config: VmConfig = serde_yaml::from_str(EMBEDDED_DEFAULTS)
        .map_err(|e| VmError::config(e, "Failed to parse embedded defaults"))?;

    // Try to find and load user config if it exists
    let user_config_path = match file {
        Some(path) => Some(path),
        None => {
            // Look for vm.yaml in current directory
            let current_dir = std::env::current_dir()
                .map_err(|e| VmError::filesystem(e, ".", "get current directory"))?;
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
pub fn handle_ports_command(fix: bool) -> VmResult<()> {
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
        .ports
        .range
        .as_ref()
        .and_then(|range| {
            if range.len() == 2 {
                Some(format!("{}-{}", range[0], range[1]))
            } else {
                None
            }
        })
        .context("No port range found in configuration")?;

    vm_println!(
        "{}",
        msg!(
            MESSAGES.config.ports_header,
            project = project_name,
            range = &current_port_range
        )
    );

    if !fix {
        // For basic ports command, just show the configuration
        return Ok(());
    }

    // Parse current range
    let current_range =
        PortRange::parse(&current_port_range).context("Failed to parse current port range")?;

    // Only check for conflicts when --fix is specified
    vm_println!("");
    vm_println!("{}", MESSAGES.config.ports_checking);

    // Check for conflicts with running Docker containers
    let conflicts = check_docker_port_conflicts(&current_range)?;

    if conflicts.is_empty() {
        vm_success!("✅ No port conflicts detected!");
        return Ok(());
    }

    warn!("Port conflicts detected:");
    for conflict in &conflicts {
        vm_println!(
            "   ⚠️  Port {} is in use by: {}",
            conflict.port,
            conflict.container
        );
    }

    // Fix conflicts by finding a new port range
    vm_println!("");
    vm_println!("{}", MESSAGES.config.ports_fixing);

    let registry = PortRegistry::load().context("Failed to load port registry")?;

    // Calculate range size from current range
    let range_size = current_range.size();

    // Find next available range
    let new_range_str = registry
        .suggest_next_range(range_size, 3000)
        .context("No available port ranges found")?;

    vm_println!(
        "{}",
        msg!(MESSAGES.config.ports_updated, range = &new_range_str)
    );

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

    vm_println!(
        "{}",
        msg!(
            MESSAGES.config.ports_resolved,
            old = &current_port_range,
            new = &new_range_str
        )
    );
    vm_println!("{}", MESSAGES.config.ports_restart_hint);

    Ok(())
}

#[derive(Debug)]
struct PortConflict {
    port: u16,
    container: String,
}

/// Check for conflicts between the given port range and running Docker containers
fn check_docker_port_conflicts(range: &PortRange) -> VmResult<Vec<PortConflict>> {
    use std::process::Command;

    let mut conflicts = Vec::new();

    // Run docker ps to get running containers with port mappings
    let output = Command::new("docker")
        .args(["ps", "--format", "{{.Names}}:{{.Ports}}"])
        .output()
        .context("Failed to run docker ps command")?;

    if !output.status.success() {
        return Err(VmError::general(
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Docker command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ),
            "Failed to check Docker port conflicts",
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
fn update_vm_config_ports(new_range: &str) -> VmResult<()> {
    use std::fs;

    let config_path = std::env::current_dir()?.join("vm.yaml");

    if !config_path.exists() {
        return Err(VmError::filesystem(
            std::io::Error::new(std::io::ErrorKind::NotFound, "vm.yaml not found"),
            "vm.yaml",
            "update configuration",
        ));
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
