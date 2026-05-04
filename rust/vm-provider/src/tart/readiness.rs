use super::{provider::TartProvider, provisioner::TartProvisioner};
use crate::VmError;
use std::thread;
use std::time::{Duration, Instant};
use vm_core::error::Result;

impl TartProvider {
    pub(super) fn is_guest_agent_ready(&self, instance_name: &str) -> bool {
        self.run_guest_agent_probe(instance_name, Duration::from_secs(3))
    }

    fn run_guest_agent_probe(&self, instance_name: &str, timeout: Duration) -> bool {
        self.tart()
            .exec_probe(instance_name, ["echo", "ready"], timeout)
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
        .ensure_workspace_mount()
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
        if !self.is_shell_config_ready(instance_name) {
            provisioner.apply_canonical_shell_config(&self.config)?;
            provisioner.apply_shell_overrides(&self.config)?;
        }

        provisioner.ensure_codex_runtime_config(&self.config)
    }

    fn is_shell_config_ready(&self, instance_name: &str) -> bool {
        self.tart().exec_probe(
            instance_name,
            [
                "sh",
                "-lc",
                "test -f \"$HOME/.zshrc\" && grep -Fq 'VM_PROJECT_PATH=' \"$HOME/.zshrc\" && grep -Fq 'VM_AI_ALIAS_REPAIR_VERSION=2' \"$HOME/.zshrc\"",
            ],
            Duration::from_secs(5),
        )
    }
}
