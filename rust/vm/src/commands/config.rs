// Configuration-related command handlers

use anyhow::Context;
use std::path::PathBuf;
use tracing::{debug, warn};

use crate::cli::{ConfigProfileSubcommand, ConfigSubcommand};
use crate::error::{VmError, VmResult};
use serde_yaml_ng as serde_yaml;
use vm_config::ports::{PortRange, PortRegistry};
use vm_config::validation::{validate_config, ValidationMode};
use vm_config::{config::VmConfig, AppConfig, ConfigOps};
use vm_core::msg;
use vm_core::{vm_print, vm_println, vm_progress, vm_success, vm_warning};
use vm_messages::messages::MESSAGES;

fn load_selected_config(
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<AppConfig> {
    AppConfig::load(config_path, profile, None).map_err(VmError::from)
}

/// Handle the `vm config validate` command.
fn handle_validate_command(config_path: Option<PathBuf>, profile: Option<String>) -> VmResult<()> {
    let config = load_selected_config(config_path, profile)?.vm;
    let report = validate_config(&config, ValidationMode::Static).map_err(|error| {
        VmError::validation(
            format!("Unexpected configuration validation error: {error}"),
            None::<String>,
        )
    })?;

    if report.has_errors() {
        return Err(VmError::validation(
            format!("Configuration is invalid:\n{report}"),
            None::<String>,
        ));
    }

    vm_success!("Configuration is valid.");
    Ok(())
}

/// Handle the `vm config show` command.
fn handle_show_command(config_path: Option<PathBuf>, profile: Option<String>) -> VmResult<()> {
    let app_config = load_selected_config(config_path, profile)?;
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

fn handle_render_command(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    instance: Option<&str>,
) -> VmResult<()> {
    let app_config = load_selected_config(config_path, profile)?;
    let config = app_config.vm;
    let provider = config.provider.as_deref().unwrap_or("docker");
    if !matches!(provider, "docker" | "podman") {
        return Err(VmError::validation(
            format!("Provider '{provider}' does not generate Docker Compose"),
            None::<String>,
        ));
    }

    let report = validate_config(&config, ValidationMode::Static).map_err(|error| {
        VmError::validation(
            format!("Unexpected configuration validation error: {error}"),
            None::<String>,
        )
    })?;
    if report.has_errors() {
        return Err(VmError::validation(report.to_string(), None::<String>));
    }

    let project_dir = match config
        .source_path
        .as_deref()
        .and_then(std::path::Path::parent)
    {
        Some(project_dir) => project_dir.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|error| VmError::filesystem(error, ".", "resolve project directory"))?,
    };
    let context = vm_provider::ProviderContext::default().with_config(app_config.global);
    let rendered =
        vm_provider::docker::render_compose_preview(&config, &project_dir, instance, &context)?;
    vm_print!("{rendered}");
    Ok(())
}

fn handle_profile_list() -> VmResult<()> {
    let config = VmConfig::load(None)?;
    let profiles = match config.profiles {
        Some(profiles) if !profiles.is_empty() => profiles,
        _ => {
            vm_println!("No profiles defined in vm.yaml.");
            return Ok(());
        }
    };

    let mut names: Vec<String> = profiles.keys().cloned().collect();
    names.sort();

    let default_profile = config.default_profile.as_deref();

    vm_println!("Profiles:");
    for name in names {
        if Some(name.as_str()) == default_profile {
            vm_println!("  * {}", name);
        } else {
            vm_println!("  - {}", name);
        }
    }

    Ok(())
}

fn handle_profile_set(name: &str) -> VmResult<()> {
    let config = VmConfig::load(None).map_err(VmError::from)?;
    let has_profile = config
        .profiles
        .as_ref()
        .map(|profiles| profiles.contains_key(name))
        .unwrap_or(false);

    if !has_profile {
        return Err(VmError::config(
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Profile '{}' not found in vm.yaml", name),
            ),
            "Invalid default profile",
        ));
    }

    let values = vec![name.to_string()];
    ConfigOps::set("default_profile", &values, false, false).map_err(VmError::from)
}

/// Handle configuration management commands
pub fn handle_config_command(
    command: &ConfigSubcommand,
    profile: Option<String>,
    config_path: Option<PathBuf>,
) -> VmResult<()> {
    match command {
        ConfigSubcommand::Validate => handle_validate_command(config_path, profile),
        ConfigSubcommand::Show => handle_show_command(config_path, profile),
        ConfigSubcommand::Render { instance } => {
            handle_render_command(config_path, profile, instance.as_deref())
        }
        ConfigSubcommand::Set {
            field,
            values,
            global,
        } => Ok(ConfigOps::set(field, values, *global, false)?),
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
        ConfigSubcommand::Profile { command } => match command {
            ConfigProfileSubcommand::Ls => handle_profile_list(),
            ConfigProfileSubcommand::Set { name } => handle_profile_set(name),
        },
        ConfigSubcommand::Ports { fix } => handle_ports_command(*fix),
        ConfigSubcommand::Clear { global } => Ok(ConfigOps::clear(*global)?),
    }
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
    vm_progress!("{}", MESSAGES.config.ports_checking);

    // Check for conflicts with running Docker containers
    let executable = config.provider.as_deref().unwrap_or("docker");
    let conflicts = check_docker_port_conflicts(executable, &current_range)?;

    if conflicts.is_empty() {
        vm_success!("No port conflicts detected");
        return Ok(());
    }

    warn!("Port conflicts detected:");
    for conflict in &conflicts {
        vm_warning!("Port {} is in use by {}", conflict.port, conflict.container);
    }

    // Fix conflicts by finding a new port range
    vm_progress!("{}", MESSAGES.config.ports_fixing);

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
fn check_docker_port_conflicts(executable: &str, range: &PortRange) -> VmResult<Vec<PortConflict>> {
    use std::process::Command;

    let mut conflicts = Vec::new();

    // Run docker ps to get running containers with port mappings
    let output = Command::new(executable)
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

#[cfg(test)]
mod tests {
    use super::{handle_validate_command, load_selected_config};

    #[test]
    fn validation_honors_explicit_config_and_does_not_modify_it() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("selected.yaml");
        let contents = b"project:\n  name: selected\nprovider: docker\n";
        std::fs::write(&config_path, contents).unwrap();

        handle_validate_command(Some(config_path.clone()), None).unwrap();

        assert_eq!(std::fs::read(config_path).unwrap(), contents);
    }

    #[test]
    fn selected_profile_is_loaded_from_explicit_config() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("selected.yaml");
        std::fs::write(
            &config_path,
            r#"
project:
  name: base
provider: docker
profiles:
  feature:
    project:
      name: feature
"#,
        )
        .unwrap();

        let loaded = load_selected_config(Some(config_path), Some("feature".to_string())).unwrap();
        assert_eq!(
            loaded
                .vm
                .project
                .and_then(|project| project.name)
                .as_deref(),
            Some("feature")
        );
    }
}
