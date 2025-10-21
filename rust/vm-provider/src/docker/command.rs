//! Docker command abstraction and builder utilities.
//!
//! This module provides a centralized interface for executing Docker commands
//! with consistent error handling, logging, and argument validation. It reduces
//! code duplication across Docker operations by providing common patterns
//! for executing Docker subcommands.

use serde::Deserialize;
use std::process::{Command, Output};
use vm_core::error::{Result, VmError};
use vm_core::{vm_dbg, vm_error};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Mount {
    source: String,
}

/// Builder for Docker commands with fluent interface and consistent error handling.
///
/// Provides a centralized way to construct and execute Docker commands with
/// proper argument validation, error handling, and logging.
#[derive(Debug, Clone)]
pub struct DockerCommand {
    subcommand: Option<String>,
    args: Vec<String>,
    #[allow(dead_code)] // Will be used for output capture in monitoring features
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
    #[allow(dead_code)] // Will be used for batch operations and plugin system
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Enable output capture for the command (useful for parsing results).
    #[allow(dead_code)] // Will be used for monitoring and health checks
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
            .map_err(|e| VmError::Internal(format!("Failed to execute Docker command: {e}")))?;

        if status.success() {
            Ok(())
        } else {
            Err(VmError::Internal(format!(
                "Docker command failed with status: {status}"
            )))
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
            .map_err(|e| VmError::Internal(format!("Failed to execute Docker command: {e}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            vm_error!("Docker command failed: {}", stderr);
            Err(VmError::Internal(format!(
                "Docker command failed with status: {}. Error: {}",
                output.status, stderr
            )))
        }
    }

    /// Execute the command and return the raw Output struct.
    ///
    /// Use this when you need access to both stdout and stderr,
    /// or need to handle non-zero exit codes manually.
    pub fn execute_raw(self) -> Result<Output> {
        let mut cmd = self.build_command()?;

        vm_dbg!("Executing Docker command (raw): {:?}", &cmd);

        cmd.output()
            .map_err(|e| VmError::Internal(format!("Failed to execute Docker command: {e}")))
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
            .map_err(|e| {
                VmError::Internal(format!(
                    "Docker daemon is not running or not accessible: {e}"
                ))
            })
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
    #[allow(dead_code)] // Utility function for debugging and manual operations
    pub fn exec_in_container(container_name: &str, command_args: &[&str]) -> Result<()> {
        let mut cmd = DockerCommand::new().subcommand("exec").arg(container_name);

        for arg in command_args {
            cmd = cmd.arg(*arg);
        }

        cmd.execute()
    }

    /// Execute a command inside a container and capture output.
    #[allow(dead_code)] // Will be used for interactive diagnostics
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
    pub fn copy(source: &str, destination: &str) -> Result<()> {
        DockerCommand::new()
            .subcommand("cp")
            .arg(source)
            .arg(destination)
            .execute()
    }

    /// Get container statistics with specified format.
    #[allow(dead_code)] // Will be used for resource monitoring
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
    #[allow(dead_code)] // Will be used for lifecycle management
    pub fn start_container(container_name: &str) -> Result<()> {
        DockerCommand::new()
            .subcommand("start")
            .arg(container_name)
            .execute()
    }

    /// Stop a container by name.
    #[allow(dead_code)] // Will be used for lifecycle management
    pub fn stop_container(container_name: &str) -> Result<()> {
        DockerCommand::new()
            .subcommand("stop")
            .arg(container_name)
            .execute()
    }

    /// Remove a container by name (with force flag).
    #[allow(dead_code)] // Will be used for cleanup operations
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

    /// Get a list of host paths mounted into the container.
    pub fn get_container_mounts(container_name: &str) -> Result<Vec<String>> {
        let output = DockerCommand::new()
            .subcommand("inspect")
            .arg("--format")
            .arg("{{json .Mounts}}")
            .arg(container_name)
            .execute_with_output()?;

        let mounts: Vec<Mount> = serde_json::from_str(&output)
            .map_err(|e| VmError::Internal(format!("Failed to parse Docker mounts: {e}")))?;

        Ok(mounts.into_iter().map(|m| m.source).collect())
    }

    /// Check if a Docker network exists by name.
    pub fn network_exists(network_name: &str) -> Result<bool> {
        let output = DockerCommand::new()
            .subcommand("network")
            .arg("ls")
            .arg("--format")
            .arg("{{.Name}}")
            .execute_with_output()?;

        Ok(output.lines().any(|line| line.trim() == network_name))
    }

    /// Create a Docker network with the specified name.
    pub fn create_network(network_name: &str) -> Result<()> {
        vm_dbg!("Creating Docker network: {}", network_name);

        DockerCommand::new()
            .subcommand("network")
            .arg("create")
            .arg(network_name)
            .execute()
            .map_err(|e| {
                VmError::Internal(format!(
                    "Failed to create Docker network '{}': {}",
                    network_name, e
                ))
            })
    }

    /// Ensure all specified networks exist, creating them if necessary.
    pub fn ensure_networks_exist(networks: &[String]) -> Result<()> {
        for network in networks {
            if !Self::network_exists(network)? {
                vm_dbg!("Network '{}' does not exist, creating it...", network);
                Self::create_network(network)?;
            } else {
                vm_dbg!("Network '{}' already exists", network);
            }
        }
        Ok(())
    }

    /// Check if a Docker image exists locally.
    ///
    /// # Arguments
    /// * `image_name` - Name of the image (e.g., "supercool:latest")
    pub fn image_exists(image_name: &str) -> Result<bool> {
        let output = DockerCommand::new()
            .subcommand("images")
            .arg("--format")
            .arg("{{.Repository}}:{{.Tag}}")
            .execute_with_output()?;

        Ok(output.lines().any(|line| line.trim() == image_name))
    }

    /// Build a custom Docker image from a Dockerfile.
    ///
    /// # Arguments
    /// * `dockerfile_path` - Path to the Dockerfile
    /// * `image_name` - Tag for the built image (e.g., "supercool:latest")
    /// * `context_dir` - Build context directory (usually parent of Dockerfile)
    pub fn build_custom_image(
        dockerfile_path: &std::path::Path,
        image_name: &str,
        context_dir: &std::path::Path,
    ) -> Result<()> {
        use vm_core::vm_info;

        vm_info!(
            "Building custom base image '{}' from {:?}...",
            image_name,
            dockerfile_path
        );
        vm_info!("This may take 5-15 minutes on first build...");

        let output = DockerCommand::new()
            .subcommand("build")
            .arg("-f")
            .arg(dockerfile_path.to_string_lossy().to_string())
            .arg("-t")
            .arg(image_name)
            .arg(context_dir.to_string_lossy().to_string())
            .execute_raw()?;

        if output.status.success() {
            vm_info!("✓ Successfully built custom base image '{}'", image_name);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(VmError::Internal(format!(
                "Failed to build custom base image '{}': {}",
                image_name, stderr
            )))
        }
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
