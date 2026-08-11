use super::{provider::TartProvider, provisioner::TartProvisioner};
use crate::{resources::SHELL_CONFIG_VERSION, VmError};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};
use vm_core::error::Result;

const SSH_IP_TIMEOUT: Duration = Duration::from_secs(2);
const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShellTransport {
    GuestAgent,
    Ssh(IpAddr),
}

impl TartProvider {
    pub(super) fn is_guest_agent_ready(&self, instance_name: &str) -> bool {
        self.run_guest_agent_probe(instance_name, Duration::from_secs(3))
    }

    fn run_guest_agent_probe(&self, instance_name: &str, timeout: Duration) -> bool {
        self.tart()
            .exec_probe(instance_name, ["echo", "ready"], timeout)
    }

    pub(super) fn shell_transport(&self, instance_name: &str) -> Option<ShellTransport> {
        if self.is_guest_agent_ready(instance_name) {
            return Some(ShellTransport::GuestAgent);
        }
        if !Self::is_macos_guest_config(&self.config) {
            return None;
        }

        let ip = self.tart().ip_address(instance_name, SSH_IP_TIMEOUT)?;
        TcpStream::connect_timeout(&SocketAddr::new(ip, 22), SSH_CONNECT_TIMEOUT)
            .ok()
            .map(|_| ShellTransport::Ssh(ip))
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
            self.tart_home(),
        )
        .ensure_workspace_mount(&self.config)
        .map_err(|e| {
            VmError::Provider(format!(
                "Tart workspace mount is not ready at '{sync_dir}'. This VM may be partially provisioned or was started without the workspace share. Recreate it with `vm remove <name> --force && vm run mac as <name>`. Mount error: {e}"
            ))
        })
    }

    pub(super) fn ensure_shell_config_ready(
        &self,
        instance_name: &str,
        sync_dir: &str,
    ) -> Result<()> {
        let provisioner = TartProvisioner::new(
            instance_name.to_string(),
            sync_dir.to_string(),
            self.tart_home(),
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
