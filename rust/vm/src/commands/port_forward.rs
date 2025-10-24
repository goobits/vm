//! Dynamic port forwarding with SSH tunneling
//!
//! This module provides ephemeral port forwarding using SSH local port forwarding.
//! Tunnels are created on-demand and can be stopped independently.
use crate::error::{VmError, VmResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use tracing::{debug, warn};
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::vm_println;
use vm_platform::platform;
use vm_provider::Provider;

/// Information about an active port forwarding tunnel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub host_port: u16,
    pub container_port: u16,
    pub container_name: String,
    pub relay_container_id: String,
    pub relay_container_name: String,
    pub created_at: String,
}

/// Manages port forwarding tunnels state
pub struct TunnelManager {
    state_file: PathBuf,
}

impl TunnelManager {
    /// Create a new tunnel manager
    pub fn new() -> VmResult<Self> {
        let config_dir = platform::user_config_dir().map_err(|e| {
            VmError::general(
                std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                "Failed to get config directory".to_string(),
            )
        })?;
        let tunnel_dir = config_dir.join("vm").join("tunnels");
        fs::create_dir_all(&tunnel_dir)
            .map_err(|e| VmError::general(e, "Failed to create tunnels directory".to_string()))?;

        let state_file = tunnel_dir.join("active.json");
        Ok(Self { state_file })
    }

    /// Load active tunnels from state file
    fn load_tunnels(&self) -> VmResult<HashMap<u16, TunnelInfo>> {
        if !self.state_file.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(&self.state_file)
            .map_err(|e| VmError::general(e, "Failed to read tunnels state".to_string()))?;

        let tunnels: HashMap<u16, TunnelInfo> = serde_json::from_str(&content)
            .map_err(|e| VmError::general(e, "Failed to parse tunnels state".to_string()))?;

        // Filter out tunnels with stopped containers
        let active_tunnels: HashMap<u16, TunnelInfo> = tunnels
            .into_iter()
            .filter(|(_, tunnel)| is_container_running(&tunnel.relay_container_id))
            .collect();

        Ok(active_tunnels)
    }

    /// Save tunnels to state file
    fn save_tunnels(&self, tunnels: &HashMap<u16, TunnelInfo>) -> VmResult<()> {
        let content = serde_json::to_string_pretty(tunnels)
            .map_err(|e| VmError::general(e, "Failed to serialize tunnels".to_string()))?;

        fs::write(&self.state_file, content)
            .map_err(|e| VmError::general(e, "Failed to write tunnels state".to_string()))?;

        Ok(())
    }

    /// Create a new port forwarding tunnel
    pub fn create_tunnel(
        &self,
        host_port: u16,
        container_port: u16,
        container_name: &str,
        _provider: &dyn Provider,
    ) -> VmResult<()> {
        let mut tunnels = self.load_tunnels()?;

        // Check if host port is already in use
        if tunnels.contains_key(&host_port) {
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::AlreadyExists, "Port already forwarded"),
                format!("Port {} is already being forwarded", host_port),
            ));
        }

        // Start relay container
        debug!(
            "Creating port forward relay: localhost:{}->{}:{}",
            host_port, container_name, container_port
        );

        let (relay_container_id, relay_container_name) =
            start_relay_container(host_port, container_port, container_name)?;

        // Store tunnel info
        let tunnel_info = TunnelInfo {
            host_port,
            container_port,
            container_name: container_name.to_string(),
            relay_container_id,
            relay_container_name: relay_container_name.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        tunnels.insert(host_port, tunnel_info);
        self.save_tunnels(&tunnels)?;

        vm_println!(
            "✓ Port forwarding active: localhost:{} → {}:{}",
            host_port,
            container_name,
            container_port
        );
        vm_println!("  Stop with: vm port stop {}", host_port);

        Ok(())
    }

    /// List active tunnels, optionally filtered by container
    pub fn list_tunnels(&self, container_filter: Option<&str>) -> VmResult<Vec<TunnelInfo>> {
        let tunnels = self.load_tunnels()?;

        let filtered: Vec<TunnelInfo> = tunnels
            .into_values()
            .filter(|t| {
                if let Some(filter) = container_filter {
                    t.container_name.contains(filter)
                } else {
                    true
                }
            })
            .collect();

        Ok(filtered)
    }

    /// Stop a specific tunnel by host port
    pub fn stop_tunnel(&self, host_port: u16) -> VmResult<()> {
        let mut tunnels = self.load_tunnels()?;

        if let Some(tunnel) = tunnels.remove(&host_port) {
            stop_relay_container(&tunnel.relay_container_id)?;
            self.save_tunnels(&tunnels)?;
            vm_println!(
                "✓ Stopped port forwarding: localhost:{} → {}:{}",
                tunnel.host_port,
                tunnel.container_name,
                tunnel.container_port
            );
            Ok(())
        } else {
            Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::NotFound, "Tunnel not found"),
                format!("No active tunnel on port {}", host_port),
            ))
        }
    }

    /// Stop all tunnels for a container
    pub fn stop_all_tunnels(&self, container_filter: Option<&str>) -> VmResult<usize> {
        let mut tunnels = self.load_tunnels()?;
        let mut stopped_count = 0;

        let to_remove: Vec<u16> = tunnels
            .iter()
            .filter(|(_, t)| {
                if let Some(filter) = container_filter {
                    t.container_name.contains(filter)
                } else {
                    true
                }
            })
            .map(|(port, _)| *port)
            .collect();

        for port in to_remove {
            if let Some(tunnel) = tunnels.remove(&port) {
                if let Err(e) = stop_relay_container(&tunnel.relay_container_id) {
                    warn!(
                        "Failed to stop relay container {}: {}",
                        tunnel.relay_container_id, e
                    );
                } else {
                    stopped_count += 1;
                    vm_println!(
                        "✓ Stopped: localhost:{} → {}:{}",
                        tunnel.host_port,
                        tunnel.container_name,
                        tunnel.container_port
                    );
                }
            }
        }

        self.save_tunnels(&tunnels)?;
        Ok(stopped_count)
    }
}

/// Start a port forwarding relay using a sidecar container
///
/// Creates a lightweight Alpine container with socat that shares the network
/// namespace of the target container and forwards traffic.
/// Returns (container_id, container_name)
fn start_relay_container(
    host_port: u16,
    container_port: u16,
    container_name: &str,
) -> VmResult<(String, String)> {
    // Use a relay container with socat that shares the network namespace
    // docker run -d --rm --network container:<name> -p <host>:<host> alpine/socat \
    //   tcp-listen:<host>,fork,reuseaddr tcp-connect:localhost:<container>

    let relay_name = format!("vm-port-forward-{}-{}", container_name, host_port);
    let network_arg = format!("--network=container:{}", container_name);
    let port_arg = format!("{}:{}", host_port, host_port);
    let listen_arg = format!("tcp-listen:{},fork,reuseaddr", host_port);
    let connect_arg = format!("tcp-connect:localhost:{}", container_port);

    let output = StdCommand::new("docker")
        .args([
            "run",
            "-d",
            "--rm",
            "--name",
            &relay_name,
            &network_arg,
            "-p",
            &port_arg,
            "alpine/socat",
            &listen_arg,
            &connect_arg,
        ])
        .output()
        .map_err(|e| VmError::general(e, "Failed to start port forward relay".to_string()))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, error.to_string()),
            "Failed to start relay container".to_string(),
        ));
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!(
        "Started port forward relay container: {} (ID: {})",
        relay_name, container_id
    );

    // Wait for container to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    Ok((container_id, relay_name))
}

/// Check if a Docker container is running
fn is_container_running(container_id: &str) -> bool {
    StdCommand::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", container_id])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let running = String::from_utf8_lossy(&output.stdout);
                Some(running.trim() == "true")
            } else {
                None
            }
        })
        .unwrap_or(false)
}

/// Stop a relay container
fn stop_relay_container(container_id: &str) -> VmResult<()> {
    let output = StdCommand::new("docker")
        .args(["stop", container_id])
        .output()
        .map_err(|e| {
            VmError::general(
                e,
                format!("Failed to stop relay container {}", container_id),
            )
        })?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, error.to_string()),
            format!("Failed to stop container {}", container_id),
        ));
    }

    Ok(())
}

/// Handle port forward command
pub fn handle_port_forward(
    provider: Box<dyn Provider>,
    mapping: &str,
    container: Option<&str>,
    config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    // Parse mapping (e.g., "8080:3000")
    let parts: Vec<&str> = mapping.split(':').collect();
    if parts.len() != 2 {
        return Err(VmError::validation(
            "Invalid port mapping format. Use: <host_port>:<container_port>".to_string(),
            Some("Example: 8080:3000".to_string()),
        ));
    }

    let host_port: u16 = parts[0].parse().map_err(|_| {
        VmError::validation(
            format!("Invalid host port: {}", parts[0]),
            Some("Port must be a number between 1-65535".to_string()),
        )
    })?;

    let container_port: u16 = parts[1].parse().map_err(|_| {
        VmError::validation(
            format!("Invalid container port: {}", parts[1]),
            Some("Port must be a number between 1-65535".to_string()),
        )
    })?;

    // Get container name
    let container_name = container.unwrap_or_else(|| {
        config
            .project
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("vm-project")
    });

    // Create tunnel
    let manager = TunnelManager::new()?;
    manager.create_tunnel(host_port, container_port, container_name, provider.as_ref())
}

/// Handle port list command
pub fn handle_port_list(
    _provider: Box<dyn Provider>,
    container: Option<&str>,
    _config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    let manager = TunnelManager::new()?;
    let tunnels = manager.list_tunnels(container)?;

    if tunnels.is_empty() {
        if let Some(filter) = container {
            vm_println!(
                "No active port forwarding tunnels for container: {}",
                filter
            );
        } else {
            vm_println!("No active port forwarding tunnels");
        }
        vm_println!("\n💡 Create a tunnel with: vm port forward <host>:<container>");
        return Ok(());
    }

    vm_println!("🔀 Active Port Forwarding Tunnels\n");
    for tunnel in tunnels {
        vm_println!(
            "  localhost:{} → {}:{}",
            tunnel.host_port,
            tunnel.container_name,
            tunnel.container_port
        );
        vm_println!(
            "    Relay: {} | Created: {}",
            tunnel.relay_container_name,
            tunnel.created_at
        );
        vm_println!("");
    }

    Ok(())
}

/// Handle port stop command
pub fn handle_port_stop(
    _provider: Box<dyn Provider>,
    port: Option<u16>,
    container: Option<&str>,
    all: bool,
    _config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    let manager = TunnelManager::new()?;

    if all || (port.is_none() && container.is_some()) {
        // Stop all tunnels (optionally filtered by container)
        let count = manager.stop_all_tunnels(container)?;
        if count == 0 {
            vm_println!("No tunnels to stop");
        } else {
            vm_println!("\n✓ Stopped {} tunnel(s)", count);
        }
    } else if let Some(host_port) = port {
        // Stop specific tunnel
        manager.stop_tunnel(host_port)?;
    } else {
        return Err(VmError::validation(
            "Must specify port number or use --all flag".to_string(),
            Some("Example: vm port stop 8080 or vm port stop --all".to_string()),
        ));
    }

    Ok(())
}
