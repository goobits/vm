//! Persistent, project-scoped tunnel state and relay lifecycle.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use vm_core::{vm_hint, vm_println, vm_success};
use vm_platform::platform;
use vm_provider::TunnelProvider;

use crate::error::{VmError, VmResult};

/// Recorded tunnel ownership and relay identity.
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

/// Recorded relays for one exact project configuration. The source environment
/// may no longer exist, so listing and closing never resolve an instance.
pub struct ProjectTunnels {
    state_file: PathBuf,
    project_config_path: String,
}

impl ProjectTunnels {
    pub fn new(config_path: &Path) -> VmResult<Self> {
        Ok(Self {
            state_file: state_file_path()?,
            project_config_path: canonical_owner(config_path)?,
        })
    }

    pub fn records(&self) -> VmResult<Vec<TunnelInfo>> {
        let mut records = load_state(&self.state_file)?
            .into_values()
            .filter(|record| record.project_config_path == self.project_config_path)
            .collect::<Vec<_>>();
        records.sort_by(|a, b| {
            (&a.name, &a.provider, &a.container_name, a.host_port).cmp(&(
                &b.name,
                &b.provider,
                &b.container_name,
                b.host_port,
            ))
        });
        Ok(records)
    }

    /// Close only the selected relay, rechecking its identity under the state lock.
    pub fn close(&self, selected: &TunnelInfo, provider: &dyn TunnelProvider) -> VmResult<()> {
        let _lock = lock_state(&self.state_file)?;
        let mut tunnels = load_state(&self.state_file)?;
        let recorded = tunnels
            .get(&selected.host_port)
            .ok_or_else(|| missing_tunnel(&selected.name))?;
        if recorded.project_config_path != self.project_config_path
            || recorded.project_config_path != selected.project_config_path
            || recorded.provider != selected.provider
            || recorded.relay_container_id != selected.relay_container_id
            || recorded.name != selected.name
            || recorded.container_name != selected.container_name
        {
            return Err(VmError::validation(
                "Tunnel changed since selection; list tunnels and retry",
                None::<String>,
            ));
        }
        if !provider.relay_is_running(&recorded.relay_container_id) {
            return Err(VmError::validation(
                "Relay is not confirmed running; its ownership record was kept",
                Some("Check the Docker or Podman engine, then retry"),
            ));
        }
        provider.stop_relay(&recorded.relay_container_id)?;
        tunnels.remove(&selected.host_port);
        save_state(&self.state_file, &tunnels)?;
        vm_success!(
            "Stopped tunnel: {}:{} -> {}:{}",
            selected.local_address,
            selected.host_port,
            selected.remote_host,
            selected.container_port
        );
        Ok(())
    }
}

fn missing_tunnel(name: &str) -> VmError {
    VmError::validation(format!("No recorded tunnel named '{name}'"), None::<String>)
}

fn canonical_owner(config_path: &Path) -> VmResult<String> {
    config_path
        .canonicalize()?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            VmError::validation("Project configuration path must be UTF-8", None::<String>)
        })
}

fn state_file_path() -> VmResult<PathBuf> {
    let config_dir = platform::user_config_dir().map_err(|e| {
        VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            "Failed to get config directory",
        )
    })?;
    let tunnel_dir = config_dir.join("vm").join("tunnels");
    fs::create_dir_all(&tunnel_dir)
        .map_err(|e| VmError::general(e, "Failed to create tunnels directory"))?;
    Ok(tunnel_dir.join("active.json"))
}

fn lock_state(state_file: &Path) -> VmResult<std::fs::File> {
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(state_file.with_extension("lock"))
        .map_err(|error| VmError::general(error, "Failed to open tunnel state lock"))?;
    file.lock_exclusive()
        .map_err(|error| VmError::general(error, "Failed to lock tunnel state"))?;
    Ok(file)
}

fn load_state(state_file: &Path) -> VmResult<HashMap<u16, TunnelInfo>> {
    if !state_file.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(state_file)
        .map_err(|e| VmError::general(e, "Failed to read tunnels state"))?;
    serde_json::from_str(&content).map_err(|e| VmError::general(e, "Failed to parse tunnels state"))
}

fn save_state(state_file: &Path, tunnels: &HashMap<u16, TunnelInfo>) -> VmResult<()> {
    let content = serde_json::to_string_pretty(tunnels)
        .map_err(|e| VmError::general(e, "Failed to serialize tunnels"))?;
    vm_core::file_system::atomic_write(state_file, content.as_bytes())
        .map_err(|e| VmError::general(e, "Failed to write tunnels state"))
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
        lock_state(&self.state_file)
    }

    /// Create a new tunnel manager
    pub fn new(
        provider: &'a dyn TunnelProvider,
        project_config_path: &std::path::Path,
        provider_name: &str,
    ) -> VmResult<Self> {
        let project_config_path = canonical_owner(project_config_path)?;
        let state_file = state_file_path()?;
        Ok(Self {
            state_file,
            project_config_path,
            provider_name: provider_name.to_string(),
            provider,
        })
    }

    /// Load active tunnels from state file
    fn load_tunnels(&self) -> VmResult<HashMap<u16, TunnelInfo>> {
        let tunnels = load_state(&self.state_file)?;

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
        save_state(&self.state_file, tunnels)
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
}

#[cfg(test)]
mod tests {
    use super::{ProjectTunnels, TunnelInfo};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use vm_provider::{TunnelProvider, VmResult};

    struct Relay {
        stopped: RefCell<Vec<String>>,
        running: bool,
    }

    impl TunnelProvider for Relay {
        fn start_tcp_relay(
            &self,
            _name: &str,
            _local: &str,
            _port: u16,
            _target: &str,
            _remote: &str,
            _remote_port: u16,
        ) -> VmResult<String> {
            unreachable!()
        }
        fn relay_is_running(&self, _id: &str) -> bool {
            self.running
        }
        fn stop_relay(&self, id: &str) -> VmResult<()> {
            self.stopped.borrow_mut().push(id.to_string());
            Ok(())
        }
    }

    fn record(owner: &str, provider: &str, port: u16) -> TunnelInfo {
        TunnelInfo {
            name: "web".into(),
            project_config_path: owner.into(),
            provider: provider.into(),
            local_address: "127.0.0.1".into(),
            remote_host: "web.internal".into(),
            host_port: port,
            container_port: 3000,
            container_name: "demo-backend-dev".into(),
            relay_container_id: format!("relay-{port}"),
            relay_container_name: format!("vm-tunnel-{port}"),
            created_at: "now".into(),
        }
    }

    #[test]
    fn project_records_and_close_do_not_require_the_source_environment() {
        let directory = tempfile::tempdir().unwrap();
        let state_file = directory.path().join("active.json");
        let mut state = HashMap::new();
        state.insert(41000, record("project-a", "docker", 41000));
        state.insert(41001, record("project-a", "podman", 41001));
        state.insert(41002, record("project-b", "docker", 41002));
        let content = serde_json::to_vec(&state).unwrap();
        std::fs::write(&state_file, content).unwrap();
        let store = ProjectTunnels {
            state_file,
            project_config_path: "project-a".into(),
        };
        assert_eq!(store.records().unwrap().len(), 2);
        let relay = Relay {
            stopped: RefCell::new(Vec::new()),
            running: true,
        };
        let selected = store.records().unwrap().remove(0);
        store.close(&selected, &relay).unwrap();
        assert_eq!(*relay.stopped.borrow(), vec![selected.relay_container_id]);
        assert_eq!(store.records().unwrap().len(), 1);
        assert_eq!(super::load_state(&store.state_file).unwrap().len(), 2);
    }

    #[test]
    fn close_rejects_changed_relay_identity() {
        let directory = tempfile::tempdir().unwrap();
        let state_file = directory.path().join("active.json");
        let mut state = HashMap::new();
        state.insert(41000, record("project-a", "docker", 41000));
        std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
        let store = ProjectTunnels {
            state_file,
            project_config_path: "project-a".into(),
        };
        let mut selected = store.records().unwrap().remove(0);
        selected.relay_container_id = "replaced".into();
        let relay = Relay {
            stopped: RefCell::new(Vec::new()),
            running: true,
        };
        assert!(store
            .close(&selected, &relay)
            .unwrap_err()
            .to_string()
            .contains("changed since selection"));
        assert!(relay.stopped.borrow().is_empty());
    }

    #[test]
    fn uncertain_relay_state_keeps_ownership_record() {
        let directory = tempfile::tempdir().unwrap();
        let state_file = directory.path().join("active.json");
        let mut state = HashMap::new();
        state.insert(41000, record("project-a", "docker", 41000));
        std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
        let store = ProjectTunnels {
            state_file,
            project_config_path: "project-a".into(),
        };
        let selected = store.records().unwrap().remove(0);
        let relay = Relay {
            stopped: RefCell::new(Vec::new()),
            running: false,
        };
        assert!(store
            .close(&selected, &relay)
            .unwrap_err()
            .to_string()
            .contains("not confirmed running"));
        assert_eq!(store.records().unwrap().len(), 1);
    }
}
