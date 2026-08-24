//! Dynamic port tunneling with SSH
//!
//! This module provides ephemeral port forwarding using SSH local port forwarding.
//! Tunnels are created on-demand and can be stopped independently.

use crate::cli::TunnelSubcommand;
use crate::error::{VmError, VmResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::warn;
use vm_config::{config::ProviderName, config::VmConfig, GlobalConfig};
use vm_core::{vm_hint, vm_println, vm_success, vm_warning};
use vm_platform::platform;
use vm_provider::{container::ContainerEngine, InstanceProvider, Provider};

pub(super) fn handle_command(
    command: TunnelSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let (provider, config, global_config) =
        super::command_context::load_provider_context(config_path, profile, None)?;
    match command {
        TunnelSubcommand::Add {
            mapping,
            environment,
        } => handle_tunnel(
            provider,
            &mapping,
            environment.as_deref(),
            config,
            global_config,
        ),
        TunnelSubcommand::Ls { environment } => {
            handle_tunnel_list(provider, environment.as_deref(), config, global_config)
        }
        TunnelSubcommand::Stop {
            port,
            environment,
            all,
        } => handle_tunnel_stop(
            provider,
            port,
            environment.as_deref(),
            all,
            config,
            global_config,
        ),
    }
}

/// Information about an active tunnel
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
    engine: ContainerEngine,
}

impl TunnelManager {
    /// Create a new tunnel manager
    pub fn new(engine: ContainerEngine) -> VmResult<Self> {
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
        Ok(Self { state_file, engine })
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
            .filter(|(_, tunnel)| self.engine.container_is_running(&tunnel.relay_container_id))
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

    /// Create a new tunnel
    pub fn create_tunnel(
        &self,
        host_port: u16,
        container_port: u16,
        container_name: &str,
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
        let relay_container_name = format!("vm-tunnel-{container_name}-{host_port}");
        let relay_container_id = self.engine.start_tcp_relay(
            &relay_container_name,
            host_port,
            container_name,
            container_port,
        )?;

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

        vm_success!(
            "Tunnel active: localhost:{} -> {}:{}",
            host_port,
            container_name,
            container_port
        );
        vm_hint!("Stop with: vm tunnel stop {host_port}");

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
            self.engine.stop_container(&tunnel.relay_container_id)?;
            self.save_tunnels(&tunnels)?;
            vm_success!(
                "Stopped tunnel: localhost:{} -> {}:{}",
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
                if let Err(e) = self.engine.stop_container(&tunnel.relay_container_id) {
                    warn!(
                        "Failed to stop relay container {}: {}",
                        tunnel.relay_container_id, e
                    );
                    vm_warning!("Failed to stop tunnel on port {}: {}", tunnel.host_port, e);
                } else {
                    stopped_count += 1;
                    vm_success!(
                        "Stopped: localhost:{} -> {}:{}",
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

/// Handle tunnel command (create a new tunnel)
fn handle_tunnel(
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
            Some("Example: vm tunnel add 8080:3000".to_string()),
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

    let _ = config;
    let container_name = provider.resolve_instance_name(container)?;

    // Create tunnel
    let manager = TunnelManager::new(container_engine(provider.as_ref())?)?;
    manager.create_tunnel(host_port, container_port, &container_name)
}

/// Handle tunnel list command
fn handle_tunnel_list(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    _config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    let manager = TunnelManager::new(container_engine(provider.as_ref())?)?;
    let resolved_container = container
        .map(|value| provider.resolve_instance_name(Some(value)))
        .transpose()?;
    let tunnels = manager.list_tunnels(resolved_container.as_deref())?;

    if tunnels.is_empty() {
        if let Some(filter) = resolved_container {
            vm_println!("No active tunnels for container: {}", filter);
        } else {
            vm_println!("No active tunnels");
        }
        vm_hint!("Create one with: vm tunnel add <host>:<container>");
        return Ok(());
    }

    vm_println!("Active tunnels");
    for tunnel in tunnels {
        vm_println!(
            "  localhost:{} -> {}:{}",
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

/// Handle tunnel stop command
fn handle_tunnel_stop(
    provider: Box<dyn Provider>,
    port: Option<u16>,
    container: Option<&str>,
    all: bool,
    _config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    let manager = TunnelManager::new(container_engine(provider.as_ref())?)?;
    let resolved_container = container
        .map(|value| provider.resolve_instance_name(Some(value)))
        .transpose()?;

    if all || (port.is_none() && resolved_container.is_some()) {
        // Stop all tunnels (optionally filtered by container)
        let count = manager.stop_all_tunnels(resolved_container.as_deref())?;
        if count == 0 {
            vm_println!("No tunnels to stop");
        } else {
            vm_success!("Stopped {count} tunnel(s)");
        }
    } else if let Some(host_port) = port {
        // Stop specific tunnel
        manager.stop_tunnel(host_port)?;
    } else {
        return Err(VmError::validation(
            "Must specify port number or use --all flag".to_string(),
            Some("Example: vm tunnel stop 8080 or vm tunnel stop --all".to_string()),
        ));
    }

    Ok(())
}

fn container_engine(provider: &dyn InstanceProvider) -> VmResult<ContainerEngine> {
    let provider_name = container_provider_name(provider.name())?;
    ContainerEngine::detect(&provider_name).map_err(Into::into)
}

fn container_provider_name(name: &str) -> VmResult<ProviderName> {
    let provider_name = ProviderName::from(name);
    if !provider_name.is_container() {
        return Err(VmError::validation(
            format!(
                "Tunnels require a Docker or Podman environment; '{}' is not supported",
                name
            ),
            None::<String>,
        ));
    }
    Ok(provider_name)
}

#[cfg(test)]
mod tests {
    use super::container_provider_name;

    #[test]
    fn tunnels_accept_only_container_providers() {
        assert_eq!(
            container_provider_name("docker").unwrap().as_str(),
            "docker"
        );
        assert_eq!(
            container_provider_name("podman").unwrap().as_str(),
            "podman"
        );
        assert!(container_provider_name("tart").is_err());
        assert!(container_provider_name("mock").is_err());
    }
}
