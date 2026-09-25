// Port inspection and conflict repair.

use crate::error::{VmError, VmResult};
use anyhow::Context;
use std::path::PathBuf;
use tracing::{debug, warn};
use vm_config::config::VmConfig;
use vm_config::ports::{PortRange, PortRegistry};
use vm_config::ConfigOps;
use vm_core::msg;
use vm_core::{vm_println, vm_progress, vm_success, vm_warning};
use vm_messages::messages::MESSAGES;

/// Handle ports command
pub fn handle_ports_command(fix: bool, config_path: Option<PathBuf>) -> VmResult<()> {
    debug!("Handling ports command: fix={}", fix);

    // Load current project configuration
    let config = VmConfig::load(config_path.clone())?;

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
    if !matches!(executable, "docker" | "podman") {
        return Err(VmError::validation(
            format!("Port conflict repair is unsupported for provider '{executable}'"),
            None::<String>,
        ));
    }
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

    // Calculate range size from current range
    let range_size = current_range.size();

    let current_dir = std::env::current_dir()?;
    let mut registry = PortRegistry::load().context("Failed to load port registry")?;
    let new_range = registry
        .replace_with_next_range(
            project_name,
            &current_range,
            range_size,
            3000,
            &current_dir.to_string_lossy(),
        )
        .context("Failed to reserve a replacement port range")?;
    let new_range_str = new_range.to_string();

    vm_println!(
        "{}",
        msg!(MESSAGES.config.ports_updated, range = &new_range_str)
    );

    // Update vm.yaml with new port range
    write_port_range(&new_range, config_path)?;

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

fn write_port_range(range: &PortRange, config_path: Option<PathBuf>) -> VmResult<()> {
    ConfigOps::set_json_at(
        "ports._range",
        &format!("[{},{}]", range.start, range.end),
        false,
        config_path,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{write_port_range, PortRange};

    #[test]
    fn port_repair_writes_selected_schema_field() {
        let directory = tempfile::tempdir().unwrap();
        let selected = directory.path().join("selected.yaml");
        std::fs::write(
            &selected,
            "project:\n  name: selected\nprovider: docker\nports:\n  _range: [3000, 3009]\n",
        )
        .unwrap();
        let range = PortRange::parse("3010-3019").unwrap();

        write_port_range(&range, Some(selected.clone())).unwrap();

        let contents = std::fs::read_to_string(selected).unwrap();
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&contents).unwrap();
        assert_eq!(
            value["ports"]["_range"],
            serde_yaml_ng::from_str::<serde_yaml_ng::Value>("[3010, 3019]").unwrap()
        );
        assert!(value.get("port_range").is_none());
    }
}
