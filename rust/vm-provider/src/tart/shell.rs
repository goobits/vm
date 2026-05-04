use super::provider::TartProvider;
use crate::{security::SecurityValidator, Provider, VmError};
use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;
use tracing::info;
use vm_cli::msg;
use vm_core::error::Result;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;

impl TartProvider {
    pub(super) fn open_shell(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;
        let state = self.get_instance_state(&instance_name)?;
        match state.as_deref() {
            Some("running") => {}
            Some(_) => {
                return Err(VmError::Provider(format!(
                    "VM {instance_name} is not running"
                )));
            }
            None => {
                return Err(VmError::Provider(format!(
                    "No such object: Tart VM {instance_name}"
                )));
            }
        }

        if !self.is_guest_agent_ready(&instance_name) {
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                vm_println!("⏳ Waiting for Tart guest agent...");
            }

            if !self.wait_for_guest_agent_ready(&instance_name, Duration::from_secs(45)) {
                return Err(VmError::Provider(format!(
                    "Tart VM '{instance_name}' is running, but the guest agent is not ready. Try again in a few seconds or run `tart restart {instance_name}`."
                )));
            }
        }

        let sync_dir = self.get_sync_directory();
        self.ensure_workspace_mount_ready(&instance_name, &sync_dir)?;
        self.ensure_shell_config_ready(&instance_name, &sync_dir)?;

        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or("zsh");

        let target_path = SecurityValidator::validate_relative_path(relative_path, &sync_dir)?;
        let target_path = target_path.to_string_lossy().into_owned();

        info!("Opening SSH session in directory: {}", target_path);
        let target_path_escaped = Self::shell_escape_single_quotes(&target_path);

        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            let user = self
                .config
                .tart
                .as_ref()
                .and_then(|tart| tart.ssh_user.as_deref())
                .unwrap_or("admin");

            vm_println!(
                "{}",
                msg!(
                    MESSAGES.service.docker_ssh_info,
                    user = user,
                    path = target_path.as_str(),
                    shell = shell
                )
            );
        }

        let shell_escaped = Self::shell_escape_single_quotes(shell);
        let ssh_command = format!(
            "export VM_TARGET_DIR='{target_path}' && cd \"$VM_TARGET_DIR\" && exec '{shell}' -il",
            target_path = target_path_escaped,
            shell = shell_escaped
        );

        let status = self
            .tart()
            .command()
            .args(["exec", "-i", "-t", &instance_name, "sh", "-c", &ssh_command])
            .status()
            .map_err(|e| VmError::Provider(format!("Exec failed: {e}")))?;

        match status.code() {
            Some(0) | Some(130) => Ok(()),
            Some(code) => Err(VmError::Provider(format!("Shell exited with code {code}"))),
            None => Err(VmError::Provider(
                "Shell terminated unexpectedly".to_string(),
            )),
        }
    }
}
