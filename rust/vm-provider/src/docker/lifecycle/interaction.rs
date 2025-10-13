//! User interaction with containers (SSH/exec/logs)
use std::io::IsTerminal;
use std::path::Path;

use super::LifecycleOperations;
use crate::{docker::UserConfig, security::SecurityValidator};
use vm_cli::msg;
use vm_core::{
    command_stream::stream_command,
    error::{Result, VmError},
    vm_println,
};
use vm_messages::messages::MESSAGES;

use super::DEFAULT_SHELL;

impl<'a> LifecycleOperations<'a> {
    #[must_use = "SSH connection results should be handled"]
    pub fn ssh_into_container(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        let workspace_path = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or(super::helpers::DEFAULT_WORKSPACE_PATH);
        let user_config = UserConfig::from_vm_config(self.config);
        let project_user = &user_config.username;
        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or(DEFAULT_SHELL);

        let target_path = SecurityValidator::validate_relative_path(relative_path, workspace_path)?;
        let target_dir = target_path.to_string_lossy();

        let tty_flag = if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            "-it"
        } else {
            "-i"
        };

        // Resolve the container name first
        let container_name = self.resolve_target_container(container)?;

        // Check if container is running before showing connection details
        // Use a quick docker inspect to check status (suppress errors)
        let status_check = duct::cmd(
            "docker",
            &["inspect", "--format", "{{.State.Running}}", &container_name],
        )
        .stderr_null()
        .read();

        let is_running = status_check.is_ok_and(|output| output.trim() == "true");

        // Only show connection details if container is running and it's interactive
        if is_running && std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.docker_ssh_info,
                    user = project_user,
                    path = target_dir.as_ref(),
                    shell = shell
                )
            );
        }

        // First check if container exists to provide better error messages
        let container_exists = duct::cmd("docker", &["inspect", &container_name])
            .stdout_null()
            .stderr_null()
            .run()
            .is_ok();

        if !container_exists {
            // Return error without printing raw Docker messages
            return Err(VmError::Internal(format!(
                "No such container: {container_name}"
            )));
        }

        // Check if container is actually running before trying to exec
        let container_running = duct::cmd(
            "docker",
            &["inspect", "-f", "{{.State.Running}}", &container_name],
        )
        .stderr_null()
        .read()
        .map(|output| output.trim() == "true")
        .unwrap_or(false);

        if !container_running {
            // Return error that will trigger the start prompt
            return Err(VmError::Internal(format!(
                "Container {container_name} is not running"
            )));
        }

        // Container is running, proceed with exec
        let result = duct::cmd(
            "docker",
            &[
                "exec",
                tty_flag,
                "-e",
                &format!("VM_TARGET_DIR={target_dir}"),
                &container_name,
                "sudo",
                "-u",
                project_user,
                "sh",
                "-c",
                &format!("cd \"$VM_TARGET_DIR\" && exec {shell}"),
            ],
        )
        .run();

        match result {
            Ok(output) => {
                // Handle exit codes gracefully
                match output.status.code() {
                    Some(0) | Some(2) => Ok(()), // Normal exit or shell builtin exit
                    Some(127) => Ok(()), // Command not found (happens when user types non-existent command then exits)
                    Some(130) => Ok(()), // Ctrl-C interrupt - treat as normal exit
                    _ => {
                        // Only return error for actual connection failures
                        Err(VmError::Command(String::from("SSH connection lost")))
                    }
                }
            }
            Err(e) => {
                // Check if the error is because the container is not running
                let error_str = e.to_string();
                if error_str.contains("is not running") {
                    // Pass through the original error so the SSH handler can detect it
                    // and offer to start the VM
                    Err(e.into())
                } else if error_str.contains("exited with code")
                    && !error_str.contains("is not running")
                {
                    // Only clean up other duct command errors that include the full command
                    // but preserve "is not running" errors for proper handling
                    if error_str.contains("exited with code 1") {
                        Err(VmError::Internal("Docker command failed".to_string()))
                    } else {
                        Err(VmError::Internal("Command execution failed".to_string()))
                    }
                } else {
                    // Pass through other errors
                    Err(e.into())
                }
            }
        }
    }

    #[must_use = "command execution results should be handled"]
    pub fn exec_in_container(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;
        let workspace_path = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or(super::helpers::DEFAULT_WORKSPACE_PATH);
        let user_config = UserConfig::from_vm_config(self.config);
        let project_user = &user_config.username;

        let mut args: Vec<&str> = vec![
            "exec",
            "-w",
            workspace_path,
            "--user",
            project_user,
            &target_container,
        ];
        let cmd_strs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&cmd_strs);
        stream_command("docker", &args)
    }

    #[must_use = "log display results should be handled"]
    pub fn show_logs(&self, container: Option<&str>) -> Result<()> {
        // Show recent logs without following (-f) to avoid hanging indefinitely
        // Use --tail to show last 50 lines and add timestamps
        let target_container = self.resolve_target_container(container)?;
        stream_command("docker", &["logs", "--tail", "50", "-t", &target_container])
            .map_err(|e| VmError::Internal(format!("Failed to show logs: {e}")))
    }
}
