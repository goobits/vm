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
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_hint, vm_println, vm_success};
use vm_platform::platform;
use vm_provider::{Provider, TunnelProvider};

pub(super) fn handle_command(
    command: TunnelSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let (provider, config, global_config) =
        super::command_context::load_provider_context(config_path, profile, None)?;
    match command {
        TunnelSubcommand::Open {
            name,
            local,
            remote,
            env,
        } => handle_tunnel(
            provider,
            &name,
            &local,
            &remote,
            env.as_deref(),
            config,
            global_config,
        ),
        TunnelSubcommand::List { env } => {
            handle_tunnel_list(provider, env.as_deref(), config, global_config)
        }
        TunnelSubcommand::Close { name, env } => {
            handle_tunnel_stop(provider, &name, env.as_deref(), config, global_config)
        }
    }
}

/// Information about an active tunnel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub name: String,
    pub host_port: u16,
    pub container_port: u16,
    pub container_name: String,
    pub relay_container_id: String,
    pub relay_container_name: String,
    pub created_at: String,
}

/// Manages port forwarding tunnels state
pub struct TunnelManager<'a> {
    state_file: PathBuf,
    provider: &'a dyn TunnelProvider,
}

impl<'a> TunnelManager<'a> {
    /// Create a new tunnel manager
    pub fn new(provider: &'a dyn TunnelProvider) -> VmResult<Self> {
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
        Ok(Self {
            state_file,
            provider,
        })
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
            .filter(|(_, tunnel)| self.provider.relay_is_running(&tunnel.relay_container_id))
            .collect();

        Ok(active_tunnels)
    }

    /// Save tunnels to state file
    fn save_tunnels(&self, tunnels: &HashMap<u16, TunnelInfo>) -> VmResult<()> {
        let content = serde_json::to_string_pretty(tunnels)
            .map_err(|e| VmError::general(e, "Failed to serialize tunnels".to_string()))?;

        vm_core::file_system::atomic_write(&self.state_file, content.as_bytes())
            .map_err(|e| VmError::general(e, "Failed to write tunnels state".to_string()))?;

        Ok(())
    }

    /// Create a new tunnel
    pub fn create_tunnel(
        &self,
        name: &str,
        host_port: u16,
        container_port: u16,
        container_name: &str,
    ) -> VmResult<()> {
        let mut tunnels = self.load_tunnels()?;

        if let Some(existing) = tunnels.values().find(|tunnel| tunnel.name == name) {
            if existing.host_port == host_port
                && existing.container_port == container_port
                && existing.container_name == container_name
            {
                vm_println!("Tunnel '{name}' is already active");
                return Ok(());
            }
            return Err(VmError::validation(
                format!("Tunnel '{name}' already uses different endpoints"),
                None::<String>,
            ));
        }

        // Check if host port is already in use
        if tunnels.contains_key(&host_port) {
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::AlreadyExists, "Port already forwarded"),
                format!("Port {} is already being forwarded", host_port),
            ));
        }

        // Start relay container
        let relay_container_name = format!("vm-tunnel-{container_name}-{host_port}");
        let relay_container_id = self.provider.start_tcp_relay(
            &relay_container_name,
            host_port,
            container_name,
            container_port,
        )?;

        // Store tunnel info
        let tunnel_info = TunnelInfo {
            name: name.to_string(),
            host_port,
            container_port,
            container_name: container_name.to_string(),
            relay_container_id: relay_container_id.clone(),
            relay_container_name: relay_container_name.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        tunnels.insert(host_port, tunnel_info);
        if let Err(error) = self.save_tunnels(&tunnels) {
            let _ = self.provider.stop_relay(&relay_container_id);
            return Err(error);
        }

        vm_success!(
            "Tunnel active: localhost:{} -> {}:{}",
            host_port,
            container_name,
            container_port
        );
        vm_hint!("Close with: vm tunnels close {name}");

        Ok(())
    }

    /// List active tunnels, optionally filtered by container
    pub fn list_tunnels(&self, container_filter: Option<&str>) -> VmResult<Vec<TunnelInfo>> {
        let tunnels = self.load_tunnels()?;

        let filtered: Vec<TunnelInfo> = tunnels
            .into_values()
            .filter(|t| {
                if let Some(filter) = container_filter {
                    t.container_name == filter
                } else {
                    true
                }
            })
            .collect();

        let mut filtered = filtered;
        filtered.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(filtered)
    }

    /// Stop a specific tunnel by host port
    pub fn stop_tunnel(&self, name: &str, container_filter: Option<&str>) -> VmResult<()> {
        let mut tunnels = self.load_tunnels()?;
        let port = tunnels.iter().find_map(|(port, tunnel)| {
            (tunnel.name == name
                && container_filter.map_or(true, |env| env == tunnel.container_name))
            .then_some(*port)
        });

        if let Some(host_port) = port {
            let tunnel = tunnels
                .get(&host_port)
                .expect("matched tunnel exists")
                .clone();
            self.provider.stop_relay(&tunnel.relay_container_id)?;
            tunnels.remove(&host_port);
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
                format!("No active tunnel named '{name}'"),
            ))
        }
    }
}

/// Handle tunnel command (create a new tunnel)
fn handle_tunnel(
    provider: Box<dyn Provider>,
    name: &str,
    local: &str,
    remote: &str,
    container: Option<&str>,
    config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(VmError::validation(
            "Tunnel names must contain only letters, digits, hyphens, or underscores",
            None::<String>,
        ));
    }
    let host_port = parse_endpoint(local, &["127.0.0.1", "localhost"])?;
    let container_port = parse_endpoint(remote, &["localhost", "127.0.0.1"])?;

    let _ = config;
    let container_name = provider.resolve_instance_name(container)?;

    // Create tunnel
    let manager = TunnelManager::new(tunnel_provider(provider.as_ref())?)?;
    manager.create_tunnel(name, host_port, container_port, &container_name)
}

fn parse_endpoint(value: &str, supported_hosts: &[&str]) -> VmResult<u16> {
    let (host, port) = value.rsplit_once(':').ok_or_else(|| {
        VmError::validation(
            format!("Invalid endpoint '{value}'"),
            Some("Use HOST:PORT".to_string()),
        )
    })?;
    if !supported_hosts.contains(&host) {
        return Err(VmError::validation(
            format!("Endpoint host '{host}' is unsupported by the current relay provider"),
            Some(format!("Supported hosts: {}", supported_hosts.join(", "))),
        ));
    }
    let port: u16 = port.parse().map_err(|_| {
        VmError::validation(
            format!("Invalid port in endpoint '{value}'"),
            None::<String>,
        )
    })?;
    if port == 0 {
        return Err(VmError::validation(
            "Port must be between 1 and 65535",
            None::<String>,
        ));
    }
    Ok(port)
}

/// Handle tunnel list command
fn handle_tunnel_list(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    _config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    let manager = TunnelManager::new(tunnel_provider(provider.as_ref())?)?;
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
        vm_hint!(
            "Create one with: vm tunnels open NAME --local localhost:8080 --remote localhost:3000"
        );
        return Ok(());
    }

    vm_println!("Active tunnels");
    for tunnel in tunnels {
        vm_println!(
            "  {}: localhost:{} -> {}:{}",
            tunnel.name,
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
    name: &str,
    container: Option<&str>,
    _config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    let manager = TunnelManager::new(tunnel_provider(provider.as_ref())?)?;
    let resolved_container = container
        .map(|value| provider.resolve_instance_name(Some(value)))
        .transpose()?;

    manager.stop_tunnel(name, resolved_container.as_deref())
}

fn tunnel_provider(provider: &dyn Provider) -> VmResult<&dyn TunnelProvider> {
    require_tunnel_provider(provider.as_tunnel_provider(), provider.name())
}

fn require_tunnel_provider<'a>(
    capability: Option<&'a dyn TunnelProvider>,
    provider_name: &str,
) -> VmResult<&'a dyn TunnelProvider> {
    capability.ok_or_else(|| {
        VmError::validation(
            format!(
                "Tunnels require a Docker or Podman environment; '{}' is not supported",
                provider_name
            ),
            None::<String>,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_endpoint, require_tunnel_provider};

    #[test]
    fn tunnels_require_an_explicit_provider_capability() {
        let error = match require_tunnel_provider(None, "tart") {
            Ok(_) => panic!("unsupported provider unexpectedly exposed tunnels"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("'tart' is not supported"));
    }

    #[test]
    fn endpoints_reject_public_bind_and_unroutable_remote_hosts() {
        assert_eq!(
            parse_endpoint("localhost:8080", &["localhost", "127.0.0.1"]).unwrap(),
            8080
        );
        assert!(parse_endpoint("0.0.0.0:8080", &["localhost", "127.0.0.1"]).is_err());
        assert!(parse_endpoint("localhost:0", &["localhost", "127.0.0.1"]).is_err());
        assert!(parse_endpoint("db.example:5432", &["localhost"]).is_err());
    }
}
