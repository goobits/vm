use super::{provider::TartProvider, provisioner::TartProvisioner};
use crate::{resources::SHELL_CONFIG_VERSION, VmError};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use vm_core::error::Result;

const SSH_IP_TIMEOUT: Duration = Duration::from_secs(2);
const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const SHELL_PROBE_CACHE_TTL: Duration = Duration::from_secs(5);
const SHELL_GUEST_AGENT_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShellTransport {
    GuestAgent,
    Ssh(IpAddr),
}

#[derive(Debug)]
pub(super) struct ShellProbeCache {
    instance: String,
    transport: ShellTransport,
    checked_at: Instant,
}

pub(super) type SharedShellProbeCache = Arc<Mutex<Option<ShellProbeCache>>>;

impl TartProvider {
    pub(super) fn is_guest_agent_ready(&self, instance_name: &str) -> bool {
        self.run_guest_agent_probe(instance_name, Duration::from_secs(3))
    }

    fn run_guest_agent_probe(&self, instance_name: &str, timeout: Duration) -> bool {
        self.tart()
            .exec_probe(instance_name, ["echo", "ready"], timeout)
    }

    pub(super) fn shell_transport(&self, instance_name: &str) -> Option<ShellTransport> {
        if let Ok(cache) = self.shell_probe_cache.lock() {
            if let Some(cached) = cache.as_ref() {
                if cached.instance == instance_name
                    && cached.checked_at.elapsed() <= SHELL_PROBE_CACHE_TTL
                {
                    return Some(cached.transport);
                }
            }
        }

        let transport = if self.run_guest_agent_probe(instance_name, SHELL_GUEST_AGENT_TIMEOUT) {
            Some(ShellTransport::GuestAgent)
        } else if Self::is_macos_guest_config(&self.config) {
            self.tart()
                .ip_address(instance_name, SSH_IP_TIMEOUT)
                .filter(|ip| {
                    TcpStream::connect_timeout(&SocketAddr::new(*ip, 22), SSH_CONNECT_TIMEOUT)
                        .is_ok()
                })
                .map(ShellTransport::Ssh)
        } else {
            None
        };

        if let Some(transport) = transport {
            if let Ok(mut cache) = self.shell_probe_cache.lock() {
                *cache = Some(ShellProbeCache {
                    instance: instance_name.to_string(),
                    transport,
                    checked_at: Instant::now(),
                });
            }
        }
        transport
    }

    pub(super) fn clear_shell_transport_cache(&self) {
        if let Ok(mut cache) = self.shell_probe_cache.lock() {
            *cache = None;
        }
    }

    pub(super) fn wait_for_guest_agent_ready(
        &self,
        instance_name: &str,
        timeout: Duration,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.run_guest_agent_probe(instance_name, Duration::from_secs(3)) {
                return true;
            }

            if Instant::now() >= deadline {
                return false;
            }

            thread::sleep(Duration::from_secs(1));
        }
    }

    pub(super) fn ensure_workspace_mount_ready(
        &self,
        instance_name: &str,
        sync_dir: &str,
    ) -> Result<()> {
        TartProvisioner::new(
            instance_name.to_string(),
            sync_dir.to_string(),
            self.command.clone(),
        )
        .ensure_workspace_mount(&self.config)
        .map_err(|e| {
            VmError::Provider(format!(
                "Tart workspace mount is not ready at '{sync_dir}'. This VM may be partially provisioned or was started without the workspace share. Recreate it with `vm remove <name> --force && vm run mac as <name>`. Mount error: {e}"
            ))
        })
    }

    pub(super) fn ensure_host_sync_mounts_ready(
        &self,
        instance_name: &str,
        sync_dir: &str,
    ) -> Result<()> {
        let provisioner = TartProvisioner::new(
            instance_name.to_string(),
            sync_dir.to_string(),
            self.command.clone(),
        );
        let Some(command) = provisioner.host_sync_mount_command(&self.config) else {
            return Ok(());
        };
        self.tart_expr(&["exec", instance_name, "sh", "-c", &command])
            .run()
            .map(|_| ())
            .map_err(|error| {
                VmError::Provider(format!("Tart host-sync mounts are not ready: {error}"))
            })
    }

    pub(super) fn shell_recovery_script(
        &self,
        instance_name: &str,
        sync_dir: &str,
    ) -> Result<String> {
        let provisioner = TartProvisioner::new(
            instance_name.to_string(),
            sync_dir.to_string(),
            self.command.clone(),
        );
        let mut commands = vec![provisioner.workspace_mount_command(&self.config)];
        if let Some(host_sync) = provisioner.host_sync_mount_command(&self.config) {
            commands.push(host_sync);
        }
        commands.extend(
            self.configured_dir_shares()?
                .into_iter()
                .filter_map(|share| {
                    share.guest_path.map(|guest_path| {
                        TartProvisioner::virtiofs_mount_command(
                            &share.tag,
                            &guest_path.display().to_string(),
                        )
                    })
                }),
        );
        let shell_config = TartProvisioner::shell_config_command(&self.config, sync_dir)?;
        commands.push(format!(
            "if ! test -f \"$HOME/.zshrc\" || ! grep -Fq 'VM_SHELL_CONFIG_VERSION={SHELL_CONFIG_VERSION}' \"$HOME/.zshrc\"; then\n{shell_config}\nfi"
        ));
        Ok(format!("set -e\n{}", commands.join("\n")))
    }

    pub(super) fn ensure_shell_config_ready(
        &self,
        instance_name: &str,
        sync_dir: &str,
    ) -> Result<()> {
        let provisioner = TartProvisioner::new(
            instance_name.to_string(),
            sync_dir.to_string(),
            self.command.clone(),
        );
        provisioner.repair_home_state()?;
        if !self.is_shell_config_ready(instance_name) {
            provisioner.apply_shell_config(&self.config)?;
        }

        provisioner.sync_codex_runtime_config(&self.config)
    }

    fn is_shell_config_ready(&self, instance_name: &str) -> bool {
        let probe = format!(
            "test -f \"$HOME/.zshrc\" && grep -Fq 'VM_PROJECT_PATH=' \"$HOME/.zshrc\" && grep -Fq 'VM_SHELL_CONFIG_VERSION={SHELL_CONFIG_VERSION}' \"$HOME/.zshrc\""
        );
        self.tart().exec_probe(
            instance_name,
            ["sh", "-lc", probe.as_str()],
            Duration::from_secs(5),
        )
    }
}
