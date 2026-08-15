use super::{provider::TartProvider, readiness::ShellTransport, ssh_identity::TartSshIdentity};
use crate::{security::SecurityValidator, shell_session, Provider, VmError};
use std::io::IsTerminal;
use std::path::Path;
use tracing::info;
use vm_core::error::Result;
use vm_core::msg;
use vm_core::{vm_println, vm_warning};
use vm_messages::messages::MESSAGES;

struct InteractiveTarget<'a> {
    instance: String,
    transport: ShellTransport,
    sync_dir: String,
    shell: &'a str,
    user: &'a str,
}

impl TartProvider {
    fn interactive_target(&self, container: Option<&str>) -> Result<InteractiveTarget<'_>> {
        let instance = self.resolve_instance_name(container)?;
        match self.get_instance_state(&instance)?.as_deref() {
            Some("running") => {}
            Some(_) => {
                return Err(VmError::Provider(format!(
                    "Tart VM '{instance}' is not running"
                )))
            }
            None => {
                return Err(VmError::Provider(format!(
                    "No such object: Tart VM '{instance}'"
                )))
            }
        }
        let transport = self.shell_transport(&instance).ok_or_else(|| {
            VmError::Provider(format!(
                "Tart VM '{instance}' is running, but neither the guest agent nor SSH is ready"
            ))
        })?;
        Ok(InteractiveTarget {
            instance,
            transport,
            sync_dir: self.get_sync_directory(),
            shell: self
                .config
                .terminal
                .as_ref()
                .and_then(|terminal| terminal.shell.as_deref())
                .unwrap_or("zsh"),
            user: self
                .config
                .tart
                .as_ref()
                .and_then(|tart| tart.ssh_user.as_deref())
                .unwrap_or("admin"),
        })
    }

    fn prepare_guest_agent(&self, target: &InteractiveTarget<'_>) -> Result<()> {
        self.ensure_workspace_mount_ready(&target.instance, &target.sync_dir)?;
        self.ensure_host_sync_mounts_ready(&target.instance, &target.sync_dir)?;
        self.ensure_configured_mounts_ready(&target.instance)?;
        self.ensure_shell_config_ready(&target.instance, &target.sync_dir)
    }

    pub(super) fn open_interactive_command(
        &self,
        container: Option<&str>,
        working_dir: &Path,
        command: &[String],
    ) -> Result<()> {
        if command.is_empty() {
            return Err(VmError::Provider(
                "Interactive command cannot be empty".into(),
            ));
        }
        let target = self.interactive_target(container)?;
        let home = if target.user == "root" {
            "/root".to_string()
        } else if Self::is_macos_guest_config(&self.config) {
            format!("/Users/{}", target.user)
        } else {
            format!("/home/{}", target.user)
        };
        let working_dir =
            SecurityValidator::validate_managed_checkout_path(working_dir, Path::new(&home))?;

        let status = match target.transport {
            ShellTransport::GuestAgent => {
                self.prepare_guest_agent(&target)?;
                let mut process = self.tart().command();
                process.args([
                    "exec",
                    "-i",
                    "-t",
                    &target.instance,
                    target.shell,
                    "-ilc",
                    "cd \"$1\"; shift; exec \"$@\"",
                    "vm-interactive",
                ]);
                process
                    .arg(&working_dir)
                    .args(command)
                    .status()
                    .map_err(|error| {
                        VmError::Provider(format!("Interactive Tart command failed: {error}"))
                    })?
            }
            ShellTransport::Ssh(ip) => {
                vm_warning!("Tart guest agent unavailable; connecting over SSH to {ip}");
                let identity = TartSshIdentity::ensure()?;
                identity.ensure_authorized(target.user, ip)?;
                let recovery = self.shell_recovery_script(&target.instance, &target.sync_dir)?;
                let command = command
                    .iter()
                    .map(|argument| shell_session::quote_posix_argument(argument))
                    .collect::<Vec<_>>()
                    .join(" ");
                identity.interactive(
                    target.user,
                    ip,
                    &format!(
                        "{recovery}\ncd {} && exec {command}",
                        shell_session::quote_posix_argument(&working_dir.to_string_lossy())
                    ),
                )?
            }
        };
        match status.code() {
            Some(0) => Ok(()),
            Some(code) => Err(VmError::Provider(format!(
                "Interactive command exited with code {code}"
            ))),
            None => Err(VmError::Provider(
                "Interactive command terminated unexpectedly".into(),
            )),
        }
    }

    pub(super) fn open_shell(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        let target = self.interactive_target(container)?;

        let target_path =
            SecurityValidator::validate_relative_path(relative_path, &target.sync_dir)?;
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
                    user = target.user,
                    path = target_path.as_str(),
                    shell = target.shell
                )
            );
        }

        let shell_quoted = shell_session::quote_posix_argument(target.shell);
        let worktree_repair = shell_session::worktree_repair_script(&target.sync_dir);
        let ssh_command = format!(
            "{worktree_repair}\nexport VM_TARGET_DIR={target_path_quoted} && cd \"$VM_TARGET_DIR\" && exec {shell_quoted} -il"
        );

        let status = match target.transport {
            ShellTransport::GuestAgent => {
                self.prepare_guest_agent(&target)?;
                self.tart()
                    .command()
                    .args([
                        "exec",
                        "-i",
                        "-t",
                        &target.instance,
                        "sh",
                        "-c",
                        &ssh_command,
                    ])
                    .status()
                    .map_err(|e| VmError::Provider(format!("Exec failed: {e}")))?
            }
            ShellTransport::Ssh(ip) => {
                vm_warning!("Tart guest agent unavailable; connecting over SSH to {ip}");
                let identity = TartSshIdentity::ensure()?;
                identity.ensure_authorized(target.user, ip)?;
                let recovery = self.shell_recovery_script(&target.instance, &target.sync_dir)?;
                identity.interactive(target.user, ip, &format!("{recovery}\n{ssh_command}"))?
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
