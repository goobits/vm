//! Docker command abstraction and builder utilities.
//!
//! This module provides a centralized interface for executing Docker commands
//! with consistent error handling, logging, and argument validation. It reduces
//! code duplication across Docker operations by providing common patterns
//! for executing Docker subcommands.

use std::process::Command;
use vm_core::error::{Result, VmError};
use vm_core::vm_dbg;

/// Builder for Docker commands with fluent interface and consistent error handling.
///
/// Provides a centralized way to construct and execute Docker commands with
/// proper argument validation, error handling, and logging.
#[derive(Debug, Clone)]
pub struct DockerCommand {
    executable: String,
    subcommand: Option<String>,
    args: Vec<String>,
}

impl DockerCommand {
    /// Create a new Docker command builder.
    pub fn new(executable: Option<&str>) -> Self {
        Self {
            executable: executable.unwrap_or("docker").to_string(),
            subcommand: None,
            args: Vec::new(),
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

    /// Execute the command and return success/failure status.
    ///
    /// Use this for commands where you only care about success/failure
    /// and don't need to capture output.
    pub fn execute(self) -> Result<()> {
        let mut cmd = self.build_command()?;

        vm_dbg!("Executing Docker command");

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

        vm_dbg!("Executing Docker command with captured output");

        let output = cmd
            .output()
            .map_err(|e| VmError::Internal(format!("Failed to execute Docker command: {e}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(VmError::Internal(format!(
                "Docker command failed with status: {}. Error: {}",
                output.status, stderr
            )))
        }
    }

    /// Build the underlying Command object.
    fn build_command(self) -> Result<Command> {
        let mut cmd = Command::new(&self.executable);

        // Enable BuildKit for all Docker commands to leverage parallel builds and cache mounts
        cmd.env("DOCKER_BUILDKIT", "1");
        cmd.env("COMPOSE_DOCKER_CLI_BUILD", "1");
        cmd.env("BUILDKIT_PROGRESS", "plain");

        if let Some(subcmd) = self.subcommand {
            cmd.arg(subcmd);
        }

        cmd.args(self.args);

        Ok(cmd)
    }
}

impl Default for DockerCommand {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Common Docker operations with pre-configured command patterns.
///
/// Provides convenience methods for frequently used Docker operations
/// with proper argument patterns and error handling.
pub struct DockerOps;

impl DockerOps {
    /// Check if Docker daemon is running by executing 'docker info'.
    pub fn check_daemon_running(executable: Option<&str>) -> Result<()> {
        let mut cmd = DockerCommand::new(executable)
            .subcommand("info")
            .build_command()?;

        let output = cmd.output().map_err(|e| {
            VmError::Internal(format!(
                "Docker daemon is not running or not accessible: {e}"
            ))
        })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(VmError::Internal(format!(
                "Docker daemon is not running or not accessible: {stderr}"
            )))
        }
    }

    /// List all containers with specified format.
    ///
    /// # Arguments
    /// * `all` - Include stopped containers (uses -a flag)
    /// * `format` - Docker format string (e.g., "{{.Names}}")
    pub fn list_containers(executable: Option<&str>, all: bool, format: &str) -> Result<String> {
        let mut cmd = DockerCommand::new(executable).subcommand("ps");

        if all {
            cmd = cmd.arg("-a");
        }

        cmd.arg("--format").arg(format).execute_with_output()
    }

    /// List VM-managed service containers owned by one environment.
    pub fn list_managed_service_containers(
        executable: Option<&str>,
        environment: &str,
    ) -> Result<Vec<String>> {
        let instance = environment.strip_suffix("-dev").unwrap_or(environment);
        let output = DockerCommand::new(executable)
            .subcommand("ps")
            .arg("-a")
            .arg("--filter")
            .arg("label=com.vm.managed=true")
            .arg("--filter")
            .arg(format!("label=com.vm.instance={instance}"))
            .arg("--format")
            .arg("{{.Names}}")
            .execute_with_output()?;
        Ok(output
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty() && *name != environment)
            .map(str::to_string)
            .collect())
    }

    /// Check if a container exists by name.
    pub fn container_exists(executable: Option<&str>, container_name: &str) -> Result<bool> {
        let output = Self::list_containers(executable, true, "{{.Names}}")?;
        Ok(output.lines().any(|line| line.trim() == container_name))
    }

    /// Check if a container is currently running.
    pub fn is_container_running(executable: Option<&str>, container_name: &str) -> Result<bool> {
        let output = Self::list_containers(executable, false, "{{.Names}}")?;
        Ok(output.lines().any(|line| line.trim() == container_name))
    }

    /// Copy files to/from a container.
    ///
    /// # Arguments
    /// * `source` - Source path (container:path or local path)
    /// * `destination` - Destination path (container:path or local path)
    pub fn copy(executable: Option<&str>, source: &str, destination: &str) -> Result<()> {
        DockerCommand::new(executable)
            .subcommand("cp")
            .arg(source)
            .arg(destination)
            .execute()
    }

    /// Start a container by name.
    pub fn start_container(executable: Option<&str>, container_name: &str) -> Result<()> {
        DockerCommand::new(executable)
            .subcommand("start")
            .arg(container_name)
            .execute_with_output()
            .map(|_| ())
    }

    /// Resume a paused container by name.
    pub fn unpause_container(executable: Option<&str>, container_name: &str) -> Result<()> {
        DockerCommand::new(executable)
            .subcommand("unpause")
            .arg(container_name)
            .execute_with_output()
            .map(|_| ())
    }

    /// Remove a container by name (with force flag).
    pub fn remove_container(
        executable: Option<&str>,
        container_name: &str,
        force: bool,
    ) -> Result<()> {
        let mut cmd = DockerCommand::new(executable).subcommand("rm");

        if force {
            cmd = cmd.arg("-f");
        }

        cmd.arg(container_name).execute()
    }

    /// Test container readiness by executing a simple command.
    pub fn test_container_readiness(executable: Option<&str>, container_name: &str) -> bool {
        DockerCommand::new(executable)
            .subcommand("exec")
            .arg(container_name)
            .arg("echo")
            .arg("ready")
            .execute()
            .is_ok()
    }

    /// Check if a Docker network exists by name.
    pub fn network_exists(executable: Option<&str>, network_name: &str) -> Result<bool> {
        let output = DockerCommand::new(executable)
            .subcommand("network")
            .arg("ls")
            .arg("--format")
            .arg("{{.Name}}")
            .execute_with_output()?;

        Ok(output.lines().any(|line| line.trim() == network_name))
    }

    /// Create a Docker network with the specified name.
    pub fn create_network(executable: Option<&str>, network_name: &str) -> Result<()> {
        vm_dbg!("Creating Docker network: {}", network_name);

        DockerCommand::new(executable)
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
    pub fn ensure_networks_exist(executable: Option<&str>, networks: &[String]) -> Result<()> {
        for network in networks {
            if !Self::network_exists(executable, network)? {
                vm_dbg!("Network '{}' does not exist, creating it...", network);
                Self::create_network(executable, network)?;
            } else {
                vm_dbg!("Network '{}' already exists", network);
            }
        }
        Ok(())
    }

    /// Build a custom Docker image from a Dockerfile.
    ///
    /// # Arguments
    /// * `executable` - The docker executable to use (e.g. "docker" or "podman")
    /// * `dockerfile_path` - Path to the Dockerfile
    /// * `image_name` - Tag for the built image (e.g., "supercool:latest")
    /// * `context_dir` - Build context directory (usually parent of Dockerfile)
    /// * `build_args` - Optional build arguments to pass to docker build (--build-arg KEY=VALUE)
    pub fn build_custom_image(
        executable: Option<&str>,
        dockerfile_path: &std::path::Path,
        image_name: &str,
        context_dir: &std::path::Path,
        build_args: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<()> {
        use vm_core::command_stream::stream_command_visible;
        use vm_core::vm_info;

        let exec_name = executable.unwrap_or("docker");

        vm_info!(
            "Building custom base image '{}' from {:?} using {}...",
            image_name,
            dockerfile_path,
            exec_name
        );
        vm_info!("This may take 5-15 minutes on first build...");

        let mut args = vec![
            "build".to_string(),
            "-f".to_string(),
            dockerfile_path.to_string_lossy().to_string(),
            "-t".to_string(),
            image_name.to_string(),
        ];

        // Add build arguments if provided
        if let Some(build_args_map) = build_args {
            for (key, value) in build_args_map {
                args.push("--build-arg".to_string());
                args.push(format!("{}={}", key, value));
            }
        }

        args.push(context_dir.to_string_lossy().to_string());

        // Stream the build output directly to the user
        stream_command_visible(exec_name, &args)?;

        vm_info!("✓ Successfully built custom base image '{}'", image_name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_command_builder() {
        let cmd = DockerCommand::new(None)
            .subcommand("ps")
            .arg("-a")
            .arg("--format")
            .arg("{{.Names}}");

        assert!(cmd.subcommand.is_some());
        assert_eq!(cmd.args.len(), 3);
    }

    #[cfg(unix)]
    #[test]
    fn managed_services_are_discovered_by_instance_label() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::TempDir::new().unwrap();
        let runtime = temp.path().join("docker");
        let log = temp.path().join("args");
        fs::write(
            &runtime,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nprintf 'demo-dev\\ndemo-postgres\\ndemo-package-edge\\n'\n",
                log.display()
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&runtime).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&runtime, permissions).unwrap();

        let services =
            DockerOps::list_managed_service_containers(Some(runtime.to_str().unwrap()), "demo-dev")
                .unwrap();

        assert_eq!(services, ["demo-postgres", "demo-package-edge"]);
        let args = fs::read_to_string(log).unwrap();
        assert!(args.contains("label=com.vm.managed=true"));
        assert!(args.contains("label=com.vm.instance=demo"));
    }
}
