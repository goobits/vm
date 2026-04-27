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
    fn shell_escape_single_quotes(input: &str) -> String {
        input.replace('\'', "'\"'\"'")
    }

    fn ensure_shell_history_writable(
        &self,
        container_name: &str,
        user_config: &UserConfig,
    ) -> Result<()> {
        // Resolve UID/GID inside the container rather than passing host values.
        // Snapshot-based boxes intentionally skip the build-time UID swap, so the
        // container's user UID can differ from the host UID (e.g. macOS host=501,
        // snapshot developer=1000). Chowning to the host UID would lock the dir
        // away from the actual shell user.
        let user = &user_config.username;
        let home = format!("/home/{user}");
        let command = format!(
            "uid=$(id -u {user}) && gid=$(id -g {user}) && mkdir -p {home}/.shell_history && touch {home}/.shell_history/zsh_history && chown -R \"$uid\":\"$gid\" {home}/.shell_history && chmod 700 {home}/.shell_history && chmod 600 {home}/.shell_history/zsh_history"
        );

        duct::cmd(
            self.executable,
            &["exec", "-u", "root", container_name, "sh", "-lc", &command],
        )
        .stdout_null()
        .stderr_null()
        .run()
        .map_err(|e| VmError::Internal(format!("Failed to prepare shell history: {e}")))?;

        Ok(())
    }

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
        let project_home = format!("/home/{project_user}");
        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or(DEFAULT_SHELL);

        let target_path = SecurityValidator::validate_relative_path(relative_path, workspace_path)?;
        let target_dir = target_path.to_string_lossy();
        let target_dir_escaped = Self::shell_escape_single_quotes(target_dir.as_ref());

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
            self.executable,
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
                    MESSAGES.service.docker_ssh_info,
                    user = project_user,
                    path = target_dir.as_ref(),
                    shell = shell
                )
            );
        }

        // First check if container exists to provide better error messages
        let container_exists = duct::cmd(self.executable, &["inspect", &container_name])
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
            self.executable,
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

        self.ensure_shell_history_writable(&container_name, &user_config)?;

        // Container is running, proceed with exec
        let result = duct::cmd(
            self.executable,
            &[
                "exec",
                tty_flag,
                &container_name,
                "sudo",
                "-Hu",
                project_user,
                "env",
                &format!("HOME={project_home}"),
                &format!("USER={project_user}"),
                &format!("LOGNAME={project_user}"),
                &format!("SHELL={shell}"),
                "sh",
                "-lc",
                &format!(
                    "export VM_TARGET_DIR='{target_dir}' && cd \"$VM_TARGET_DIR\" && exec \"$SHELL\" -il",
                    target_dir = target_dir_escaped
                ),
            ],
        )
        .env("DOCKER_CLI_HINTS", "false")
        .unchecked() // Allow all exit codes - we'll handle them below
        .run();

        match result {
            Ok(output) => {
                // Handle exit codes gracefully
                match output.status.code() {
                    Some(0) => Ok(()),
                    Some(130) => Ok(()), // Ctrl-C interrupt - treat as normal exit
                    Some(code) => Err(VmError::Command(format!("Shell exited with code {code}"))),
                    None => Err(VmError::Command(String::from(
                        "Shell terminated unexpectedly",
                    ))),
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
        let project_home = format!("/home/{project_user}");
        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or(DEFAULT_SHELL);
        let workspace_escaped = Self::shell_escape_single_quotes(workspace_path);

        let mut args: Vec<String> = vec![
            "exec".to_string(),
            target_container,
            "sudo".to_string(),
            "-Hu".to_string(),
            project_user.to_string(),
            "env".to_string(),
            format!("HOME={project_home}"),
            format!("USER={project_user}"),
            format!("LOGNAME={project_user}"),
            format!("SHELL={shell}"),
            shell.to_string(),
            "-ilc".to_string(),
            format!("cd '{workspace_escaped}' && exec \"$@\""),
            "vm-exec".to_string(),
        ];
        args.extend(cmd.iter().cloned());
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        stream_command(self.executable, &arg_refs)
    }

    #[must_use = "log display results should be handled"]
    pub fn show_logs(&self, container: Option<&str>) -> Result<()> {
        // Show recent logs without following (-f) to avoid hanging indefinitely
        // Use --tail to show last 50 lines and add timestamps
        let target_container = self.resolve_target_container(container)?;
        stream_command(
            self.executable,
            &["logs", "--tail", "50", "-t", &target_container],
        )
        .map_err(|e| VmError::Internal(format!("Failed to show logs: {e}")))
    }

    #[must_use = "log display results should be handled"]
    pub fn show_logs_extended(
        &self,
        container: Option<&str>,
        follow: bool,
        tail: usize,
        service: Option<&str>,
        _config: &vm_config::config::VmConfig,
    ) -> Result<()> {
        // If service flag is set, map to container name
        let target_container = if let Some(svc) = service {
            self.map_service_to_container(svc)?
        } else {
            self.resolve_target_container(container)?
        };

        // Build docker logs command
        let mut args = vec!["logs"];

        if follow {
            args.push("--follow");
        }

        args.push("--tail");
        let tail_str = tail.to_string();
        args.push(&tail_str);

        args.push("--timestamps");
        args.push(&target_container);

        // Show helpful header
        if follow {
            vm_println!(
                "📜 Following logs for '{}' (press Ctrl+C to stop)",
                target_container
            );
            vm_println!("──────────────────────────────────────────\n");
        } else {
            vm_println!("📜 Logs for '{}' (last {} lines)", target_container, tail);
            vm_println!("──────────────────────────────────────────\n");
        }

        stream_command(self.executable, &args)
            .map_err(|e| VmError::Internal(format!("Failed to show logs: {e}")))
    }

    /// Map service names to global container names
    fn map_service_to_container(&self, service: &str) -> Result<String> {
        let container = match service {
            "postgresql" | "postgres" => "vm-postgres-global",
            "redis" => "vm-redis-global",
            "mongodb" | "mongo" => "vm-mongodb-global",
            "mysql" => "vm-mysql-global",
            _ => {
                return Err(VmError::Internal(format!(
                    "Unknown service: '{}'. Available: postgresql, redis, mongodb, mysql",
                    service
                )))
            }
        };

        // Check if container exists
        let check = std::process::Command::new(self.executable)
            .args(["inspect", container])
            .output();

        match check {
            Ok(output) if output.status.success() => Ok(container.to_string()),
            _ => Err(VmError::Internal(format!(
                "Service '{}' container not found ({}). Start the VM to enable {} service",
                service, container, service
            ))),
        }
    }
}
