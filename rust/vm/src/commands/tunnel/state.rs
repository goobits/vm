//! Persistent, project-scoped tunnel state and relay lifecycle.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use vm_core::{vm_hint, vm_println, vm_success};
use vm_platform::platform;
use vm_provider::TunnelProvider;

use crate::error::{VmError, VmResult};

/// Information about an active tunnel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub name: String,
    pub project_config_path: String,
    pub provider: String,
    pub local_address: String,
    pub remote_host: String,
    pub host_port: u16,
    pub container_port: u16,
    pub container_name: String,
    pub relay_container_id: String,
    pub relay_container_name: String,
    pub created_at: String,
}

/// Manages port forwarding tunnels state
pub struct TunnelManager<'a> {
    pub(super) state_file: PathBuf,
    pub(super) project_config_path: String,
    pub(super) provider_name: String,
    pub(super) provider: &'a dyn TunnelProvider,
}

impl<'a> TunnelManager<'a> {
    fn lock_state(&self) -> VmResult<std::fs::File> {
        let lock_path = self.state_file.with_extension("lock");
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| VmError::general(error, "Failed to open tunnel state lock"))?;
        file.lock_exclusive()
            .map_err(|error| VmError::general(error, "Failed to lock tunnel state"))?;
        Ok(file)
    }

    /// Create a new tunnel manager
    pub fn new(
        provider: &'a dyn TunnelProvider,
        project_config_path: &std::path::Path,
        provider_name: &str,
    ) -> VmResult<Self> {
        let project_config_path = project_config_path
            .canonicalize()?
            .to_string_lossy()
            .into_owned();
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
            project_config_path,
            provider_name: provider_name.to_string(),
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
            .filter(|(_, tunnel)| {
                tunnel.provider != self.provider_name
                    || self.provider.relay_is_running(&tunnel.relay_container_id)
            })
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
        local_address: &str,
        host_port: u16,
        remote_host: &str,
        container_port: u16,
        container_name: &str,
    ) -> VmResult<()> {
        let _lock = self.lock_state()?;
        let mut tunnels = self.load_tunnels()?;

        if let Some(existing) = tunnels.values().find(|tunnel| {
            tunnel.name == name
                && tunnel.project_config_path == self.project_config_path
                && tunnel.provider == self.provider_name
                && tunnel.container_name == container_name
        }) {
            if existing.local_address == local_address
                && existing.remote_host == remote_host
                && existing.host_port == host_port
                && existing.container_port == container_port
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
            local_address,
            host_port,
            container_name,
            remote_host,
            container_port,
        )?;

        // Store tunnel info
        let tunnel_info = TunnelInfo {
            name: name.to_string(),
            project_config_path: self.project_config_path.clone(),
            provider: self.provider_name.clone(),
            local_address: local_address.to_string(),
            remote_host: remote_host.to_string(),
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
            "Tunnel active: {}:{} -> {}:{} via {}",
            local_address,
            host_port,
            remote_host,
            container_port,
            container_name
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
                t.project_config_path == self.project_config_path
                    && t.provider == self.provider_name
            })
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
        let _lock = self.lock_state()?;
        let mut tunnels = self.load_tunnels()?;
        let port = tunnels.iter().find_map(|(port, tunnel)| {
            (tunnel.name == name
                && tunnel.project_config_path == self.project_config_path
                && tunnel.provider == self.provider_name
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
                "Stopped tunnel: {}:{} -> {}:{}",
                tunnel.local_address,
                tunnel.host_port,
                tunnel.remote_host,
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
