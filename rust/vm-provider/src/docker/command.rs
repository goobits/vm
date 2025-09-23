//! Docker command abstraction and builder utilities.
//!
//! This module provides a centralized interface for executing Docker commands
//! with consistent error handling, logging, and argument validation. It reduces
//! code duplication across Docker operations by providing common patterns
//! for executing Docker subcommands.

use anyhow::{Context, Result};
use std::process::{Command, Output};
use vm_common::{vm_dbg, vm_error};

/// Builder for Docker commands with fluent interface and consistent error handling.
///
/// Provides a centralized way to construct and execute Docker commands with
/// proper argument validation, error handling, and logging.
///
/// # Examples
///
/// ```ignore
/// use vm_provider::docker::command::DockerCommand;
///
/// // Check if container exists
/// let exists = DockerCommand::new()
///     .subcommand("ps")
///     .args(&["-a", "--format", "{{.Names}}"])
///     .execute_with_output()?
///     .contains("my-container");
///
/// // Execute command in container
/// DockerCommand::new()
///     .subcommand("exec")
///     .args(&["my-container", "echo", "hello"])
///     .execute()?;
/// ```
#[derive(Debug, Clone)]
#[allow(dead_code)] // Infrastructure for future refactoring
pub struct DockerCommand {
    subcommand: Option<String>,
    args: Vec<String>,
    #[allow(dead_code)]
    capture_output: bool,
}

impl DockerCommand {
    /// Create a new Docker command builder.
    pub fn new() -> Self {
        Self {
            subcommand: None,
            args: Vec::new(),
            capture_output: false,
        }
    }

    /// Set the Docker subcommand (e.g., "ps", "exec", "cp").
    pub fn subcommand<S: Into<String>>(mut self, cmd: S) -> Self {
        self.subcommand = Some(cmd.into());
        self
    }

    /// Add a single argument to the command.
    pub fn arg<S: Into<String>>(mut self, arg: S) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments to the command.
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Enable output capture for the command (useful for parsing results).
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn capture_output(mut self) -> Self {
        self.capture_output = true;
        self
    }

    /// Execute the command and return success/failure status.
    ///
    /// Use this for commands where you only care about success/failure
    /// and don't need to capture output.
    pub fn execute(self) -> Result<()> {
        let mut cmd = self.build_command()?;

        vm_dbg!("Executing Docker command: {:?}", &cmd);

        let status = cmd
            .status()
            .with_context(|| "Failed to execute Docker command")?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Docker command failed with status: {}",
                status
            ))
        }
    }

    /// Execute the command and return the output.
    ///
    /// Use this for commands where you need to parse or examine the output.
    pub fn execute_with_output(self) -> Result<String> {
        let mut cmd = self.build_command()?;

        vm_dbg!("Executing Docker command with output: {:?}", &cmd);

        let output = cmd
            .output()
            .with_context(|| "Failed to execute Docker command")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            vm_error!("Docker command failed: {}", stderr);
            Err(anyhow::anyhow!(
                "Docker command failed with status: {}. Error: {}",
                output.status,
                stderr
            ))
        }
    }

    /// Execute the command and return the raw Output struct.
    ///
    /// Use this when you need access to both stdout and stderr,
    /// or need to handle non-zero exit codes manually.
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn execute_raw(self) -> Result<Output> {
        let mut cmd = self.build_command()?;

        vm_dbg!("Executing Docker command (raw): {:?}", &cmd);

        cmd.output()
            .with_context(|| "Failed to execute Docker command")
    }

    /// Build the underlying Command object.
    fn build_command(self) -> Result<Command> {
        let mut cmd = Command::new("docker");

        if let Some(subcmd) = self.subcommand {
            cmd.arg(subcmd);
        }

        cmd.args(self.args);

        Ok(cmd)
    }
}

impl Default for DockerCommand {
    fn default() -> Self {
        Self::new()
    }
}

/// Common Docker operations with pre-configured command patterns.
///
/// Provides convenience methods for frequently used Docker operations
/// with proper argument patterns and error handling.
pub struct DockerOps;

impl DockerOps {
    /// Check if Docker daemon is running by executing 'docker info'.
    pub fn check_daemon_running() -> Result<()> {
        DockerCommand::new()
            .subcommand("info")
            .execute()
            .with_context(|| "Docker daemon is not running or not accessible")
    }

    /// List all containers with specified format.
    ///
    /// # Arguments
    /// * `all` - Include stopped containers (uses -a flag)
    /// * `format` - Docker format string (e.g., "{{.Names}}")
    pub fn list_containers(all: bool, format: &str) -> Result<String> {
        let mut cmd = DockerCommand::new().subcommand("ps");

        if all {
            cmd = cmd.arg("-a");
        }

        cmd.arg("--format").arg(format).execute_with_output()
    }

    /// Check if a container exists by name.
    pub fn container_exists(container_name: &str) -> Result<bool> {
        let output = Self::list_containers(true, "{{.Names}}")?;
        Ok(output.lines().any(|line| line.trim() == container_name))
    }

    /// Check if a container is currently running.
    pub fn is_container_running(container_name: &str) -> Result<bool> {
        let output = Self::list_containers(false, "{{.Names}}")?;
        Ok(output.lines().any(|line| line.trim() == container_name))
    }

    /// Execute a command inside a container.
    ///
    /// # Arguments
    /// * `container_name` - Name of the container
    /// * `command_args` - Command and arguments to execute
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn exec_in_container(container_name: &str, command_args: &[&str]) -> Result<()> {
        let mut cmd = DockerCommand::new().subcommand("exec").arg(container_name);

        for arg in command_args {
            cmd = cmd.arg(*arg);
        }

        cmd.execute()
    }

    /// Execute a command inside a container and capture output.
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn exec_in_container_with_output(
        container_name: &str,
        command_args: &[&str],
    ) -> Result<String> {
        let mut cmd = DockerCommand::new().subcommand("exec").arg(container_name);

        for arg in command_args {
            cmd = cmd.arg(*arg);
        }

        cmd.execute_with_output()
    }

    /// Copy files to/from a container.
    ///
    /// # Arguments
    /// * `source` - Source path (container:path or local path)
    /// * `destination` - Destination path (container:path or local path)
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn copy(source: &str, destination: &str) -> Result<()> {
        DockerCommand::new()
            .subcommand("cp")
            .arg(source)
            .arg(destination)
            .execute()
    }

    /// Get container statistics with specified format.
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn stats(container_name: &str, format: &str) -> Result<String> {
        DockerCommand::new()
            .subcommand("stats")
            .arg("--no-stream")
            .arg("--format")
            .arg(format)
            .arg(container_name)
            .execute_with_output()
    }

    /// Start a container by name.
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn start_container(container_name: &str) -> Result<()> {
        DockerCommand::new()
            .subcommand("start")
            .arg(container_name)
            .execute()
    }

    /// Stop a container by name.
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn stop_container(container_name: &str) -> Result<()> {
        DockerCommand::new()
            .subcommand("stop")
            .arg(container_name)
            .execute()
    }

    /// Remove a container by name (with force flag).
    #[allow(dead_code)] // Infrastructure for future refactoring
    pub fn remove_container(container_name: &str, force: bool) -> Result<()> {
        let mut cmd = DockerCommand::new().subcommand("rm");

        if force {
            cmd = cmd.arg("-f");
        }

        cmd.arg(container_name).execute()
    }

    /// Test container readiness by executing a simple command.
    pub fn test_container_readiness(container_name: &str) -> bool {
        DockerCommand::new()
            .subcommand("exec")
            .arg(container_name)
            .arg("echo")
            .arg("ready")
            .execute()
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_command_builder() {
        let cmd = DockerCommand::new()
            .subcommand("ps")
            .arg("-a")
            .arg("--format")
            .arg("{{.Names}}")
            .capture_output();

        assert!(cmd.subcommand.is_some());
        assert_eq!(cmd.args.len(), 3);
        assert!(cmd.capture_output);
    }

    #[test]
    fn test_docker_command_chaining() {
        let cmd = DockerCommand::new()
            .subcommand("exec")
            .arg("container")
            .arg("echo")
            .arg("hello");

        assert_eq!(cmd.args, vec!["container", "echo", "hello"]);
    }
}
