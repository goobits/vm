//! User interaction with containers (SSH/exec/logs)
use std::io::IsTerminal;
use std::path::Path;

use super::LifecycleOperations;
use crate::{container::UserConfig, security::SecurityValidator, shell_session};
use vm_core::msg;
use vm_core::{
    command_stream::stream_command_visible,
    error::{Result, VmError},
    vm_println, vm_progress,
};
use vm_messages::messages::MESSAGES;

use super::DEFAULT_SHELL;

impl<'a> LifecycleOperations<'a> {
    #[must_use = "interactive command results should be handled"]
    pub fn exec_interactive_in_container(
        &self,
        container: Option<&str>,
        working_dir: &Path,
        command: &[String],
    ) -> Result<()> {
        if command.is_empty() {
            return Err(VmError::Internal(
                "Interactive command cannot be empty".into(),
            ));
        }
        let target_container = self.resolve_target_container(container)?;
        let user_config = UserConfig::from_vm_config(self.config);
        let project_user = &user_config.username;
        let project_home = if project_user == "root" {
            "/root".to_string()
        } else {
            format!("/home/{project_user}")
        };
        let working_dir = SecurityValidator::validate_managed_checkout_path(
            working_dir,
            Path::new(&project_home),
        )?;
        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.shell.as_deref())
            .unwrap_or(DEFAULT_SHELL);
        Self::repair_home_state(self.executable, &target_container, &user_config)?;

        let tty_flag = if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            "-it"
        } else {
            "-i"
        };
        let mut arguments = vec![
            "exec".to_string(),
            tty_flag.to_string(),
            target_container,
            "sudo".to_string(),
            "-Hu".to_string(),
            project_user.to_string(),
            "env".to_string(),
            format!("HOME={project_home}"),
            format!("USER={project_user}"),
            format!("LOGNAME={project_user}"),
            format!("SHELL={shell}"),
            "VM_MANAGED_GUEST=1".to_string(),
            shell.to_string(),
            "-ilc".to_string(),
            "cd \"$1\"; shift; exec \"$@\"".to_string(),
            "vm-interactive".to_string(),
            working_dir.to_string_lossy().into_owned(),
        ];
        arguments.extend(command.iter().cloned());
        let argument_refs = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        duct::cmd(self.executable, &argument_refs)
            .run()
            .map(|_| ())
            .map_err(|_| VmError::Internal("Interactive guest command failed".into()))
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
        let target_dir_quoted = shell_session::quote_posix_argument(target_dir.as_ref());
        let worktree_repair = shell_session::worktree_repair_script(workspace_path);

        let tty_flag = if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            "-it"
        } else {
            "-i"
        };

        // Resolve the container name first
        let container_name = self.resolve_target_container(container)?;

        // Confirm the target once. The command handler already made it ready;
        // this final check preserves direct provider-call diagnostics without
        // repeating separate existence and running inspections.
        let status = duct::cmd(
            self.executable,
            &["inspect", "--format", "{{.State.Running}}", &container_name],
        )
        .stderr_null()
        .read()
        .map_err(|_| VmError::Internal(format!("No such container: {container_name}")))?;
        if status.trim() != "true" {
            return Err(VmError::Internal(format!(
                "Container {container_name} is not running"
            )));
        }

        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
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

        Self::repair_home_state(self.executable, &container_name, &user_config)?;

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
                "VM_MANAGED_GUEST=1",
                "sh",
                "-lc",
                &format!(
                    "{worktree_repair}\nexport VM_TARGET_DIR={target_dir_quoted} && cd \"$VM_TARGET_DIR\" && exec \"$SHELL\" -il"
                ),
            ],
        )
        .env("DOCKER_CLI_HINTS", "false")
        .unchecked() // Allow all exit codes - we'll handle them below
        .run();

        match result {
            // Interactive shell exit code reflects the last command the user ran,
            // not whether the session itself succeeded. Treat any clean return from
            // docker exec as a normal end of session.
            Ok(_) => Ok(()),
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
                        Err(VmError::Internal("Container command failed".to_string()))
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
        let args = self.container_exec_args(container, cmd, false)?;
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        stream_command_visible(self.executable, &arg_refs)
    }

    #[must_use = "command execution results should be handled"]
    pub fn exec_in_container_with_stdin(
        &self,
        container: Option<&str>,
        cmd: &[String],
        input: &[u8],
    ) -> Result<()> {
        let args = self.container_exec_args(container, cmd, true)?;
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        duct::cmd(self.executable, &arg_refs)
            .stdin_bytes(input.to_vec())
            .run()
            .map(|_| ())
            .map_err(|_| VmError::Internal("Guest command with standard input failed".into()))
    }

    pub fn exec_in_container_output(
        &self,
        container: Option<&str>,
        cmd: &[String],
    ) -> Result<String> {
        let args = self.container_exec_args(container, cmd, false)?;
        let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        duct::cmd(self.executable, &arg_refs)
            .stderr_capture()
            .read()
            .map_err(Into::into)
    }

    fn container_exec_args(
        &self,
        container: Option<&str>,
        cmd: &[String],
        attach_stdin: bool,
    ) -> Result<Vec<String>> {
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
        let workspace_quoted = shell_session::quote_posix_argument(workspace_path);
        let worktree_repair = shell_session::worktree_repair_script(workspace_path);

        Self::repair_home_state(self.executable, &target_container, &user_config)?;

        let mut args: Vec<String> = vec!["exec".to_string()];
        if attach_stdin {
            args.push("-i".to_string());
        }
        args.extend([
            target_container,
            "sudo".to_string(),
            "-Hu".to_string(),
            project_user.to_string(),
            "env".to_string(),
            format!("HOME={project_home}"),
            format!("USER={project_user}"),
            format!("LOGNAME={project_user}"),
            format!("SHELL={shell}"),
            "VM_MANAGED_GUEST=1".to_string(),
            shell.to_string(),
            "-ilc".to_string(),
            format!("{worktree_repair}\ncd {workspace_quoted} && exec \"$@\""),
            "vm-exec".to_string(),
        ]);
        args.extend(cmd.iter().cloned());
        Ok(args)
    }

    #[must_use = "log display results should be handled"]
    pub fn show_logs(&self, container: Option<&str>) -> Result<()> {
        // Show recent logs without following (-f) to avoid hanging indefinitely
        // Use --tail to show last 50 lines and add timestamps
        let target_container = self.resolve_target_container(container)?;
        stream_command_visible(
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
            vm_progress!("Following logs for '{target_container}' (press Ctrl+C to stop)");
        } else {
            vm_progress!("Showing the last {tail} log lines for '{target_container}'");
        }

        stream_command_visible(self.executable, &args)
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
