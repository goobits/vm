use super::{provider::TartProvider, readiness::ShellTransport};
use crate::{security::SecurityValidator, shell_session, Provider, VmError};
use std::io::IsTerminal;
use std::net::IpAddr;
use std::path::Path;
use std::process::Command;
use tracing::info;
use vm_core::error::Result;
use vm_core::msg;
use vm_core::{vm_println, vm_warning};
use vm_messages::messages::MESSAGES;

fn direct_ssh_args(user: &str, ip: IpAddr) -> Vec<String> {
    vec![
        "-t".to_string(),
        "-o".to_string(),
        "ConnectTimeout=5".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=30".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=no".to_string(),
        "-o".to_string(),
        "UserKnownHostsFile=/dev/null".to_string(),
        "-o".to_string(),
        "LogLevel=ERROR".to_string(),
        format!("{user}@{ip}"),
    ]
}

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

        let transport = self.shell_transport(&instance_name).ok_or_else(|| {
            VmError::Provider(format!(
                "Tart VM '{instance_name}' is running, but neither the guest agent nor SSH is ready"
            ))
        })?;
        let sync_dir = self.get_sync_directory();
        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or("zsh");

        let target_path = SecurityValidator::validate_relative_path(relative_path, &sync_dir)?;
        let target_path = target_path.to_string_lossy().into_owned();

        info!("Opening SSH session in directory: {}", target_path);
        let target_path_quoted = shell_session::quote_posix_argument(&target_path);

        let user = self
            .config
            .tart
            .as_ref()
            .and_then(|tart| tart.ssh_user.as_deref())
            .unwrap_or("admin");

        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
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

        let shell_quoted = shell_session::quote_posix_argument(shell);
        let worktree_repair = shell_session::worktree_repair_script(&sync_dir);
        let ssh_command = format!(
            "{worktree_repair}\nexport VM_TARGET_DIR={target_path_quoted} && cd \"$VM_TARGET_DIR\" && exec {shell_quoted} -il"
        );

        let status = match transport {
            ShellTransport::GuestAgent => {
                self.ensure_workspace_mount_ready(&instance_name, &sync_dir)?;
                self.ensure_configured_mounts_ready(&instance_name)?;
                self.ensure_shell_config_ready(&instance_name, &sync_dir)?;
                self.tart()
                    .command()
                    .args(["exec", "-i", "-t", &instance_name, "sh", "-c", &ssh_command])
                    .status()
                    .map_err(|e| VmError::Provider(format!("Exec failed: {e}")))?
            }
            ShellTransport::Ssh(ip) => {
                vm_warning!("Tart guest agent unavailable; connecting over SSH to {ip}");
                Command::new("ssh")
                    .args(direct_ssh_args(user, ip))
                    .arg(&ssh_command)
                    .status()
                    .map_err(|e| VmError::Provider(format!("SSH failed: {e}")))?
            }
        };

        match status.code() {
            Some(0) | Some(130) => Ok(()),
            Some(code) => Err(VmError::Provider(format!("Shell exited with code {code}"))),
            None => Err(VmError::Provider(
                "Shell terminated unexpectedly".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::direct_ssh_args;

    #[test]
    fn direct_ssh_uses_ephemeral_host_key_policy() {
        let args = direct_ssh_args("admin", "192.168.64.37".parse().unwrap());

        assert!(args.iter().any(|arg| arg == "StrictHostKeyChecking=no"));
        assert!(args.iter().any(|arg| arg == "UserKnownHostsFile=/dev/null"));
        assert_eq!(args.last().map(String::as_str), Some("admin@192.168.64.37"));
    }
}
