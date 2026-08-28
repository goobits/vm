//! Container-engine command abstraction and builder utilities.
//!
//! This module provides one interface for Docker-compatible engine commands
//! with consistent error handling, logging, and argument validation.

use std::process::Command;
use vm_core::error::{Result, VmError};
use vm_core::vm_dbg;

use super::engine::ContainerRuntime;

/// Builder for container-engine commands with consistent error handling.
///
/// Provides one way to construct and execute engine commands.
#[derive(Debug, Clone)]
pub struct ContainerCommand {
    runtime: ContainerRuntime,
    subcommand: Option<String>,
    args: Vec<String>,
}

impl ContainerCommand {
    /// Create a new container-engine command builder.
    pub(crate) fn new(runtime: &ContainerRuntime) -> Self {
        Self {
            runtime: runtime.clone(),
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

        vm_dbg!("Executing container-engine command");

        let status = cmd
            .status()
            .map_err(|e| VmError::Internal(format!("Failed to execute container command: {e}")))?;

        if status.success() {
            Ok(())
        } else {
            Err(VmError::Internal(format!(
                "Container command failed with status: {status}"
            )))
        }
    }

    /// Execute the command and return the output.
    ///
    /// Use this for commands where you need to parse or examine the output.
    pub fn execute_with_output(self) -> Result<String> {
        let mut cmd = self.build_command()?;

        vm_dbg!("Executing container-engine command with captured output");

        let output = cmd
            .output()
            .map_err(|e| VmError::Internal(format!("Failed to execute container command: {e}")))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(VmError::Internal(format!(
                "Container command failed with status: {}. Error: {}",
                output.status, stderr
            )))
        }
    }

    /// Build the underlying Command object.
    fn build_command(self) -> Result<Command> {
        let mut cmd = Command::new(self.runtime.executable());

        if matches!(self.runtime.engine(), super::ContainerEngine::Docker) {
            cmd.env("DOCKER_BUILDKIT", "1");
            cmd.env("COMPOSE_DOCKER_CLI_BUILD", "1");
            cmd.env("BUILDKIT_PROGRESS", "plain");
        }

        if let Some(subcmd) = self.subcommand {
            cmd.arg(subcmd);
        }

        cmd.args(self.args);

        Ok(cmd)
    }
}

/// Common Docker operations with pre-configured command patterns.
///
/// Provides convenience methods for frequently used Docker operations
/// with proper argument patterns and error handling.
pub struct ContainerOps;

impl ContainerOps {
    /// List all containers with specified format.
    ///
    /// # Arguments
    /// * `all` - Include stopped containers (uses -a flag)
    /// * `format` - Docker format string (e.g., "{{.Names}}")
    pub fn list_containers(runtime: &ContainerRuntime, all: bool, format: &str) -> Result<String> {
        let mut cmd = ContainerCommand::new(runtime).subcommand("ps");

        if all {
            cmd = cmd.arg("-a");
        }

        cmd.arg("--format").arg(format).execute_with_output()
    }

    /// List VM-managed service containers owned by one environment.
    pub fn list_managed_service_containers(
        runtime: &ContainerRuntime,
        environment: &str,
    ) -> Result<Vec<String>> {
        let instance = environment.strip_suffix("-dev").unwrap_or(environment);
        let output = ContainerCommand::new(runtime)
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
    pub fn container_exists(runtime: &ContainerRuntime, container_name: &str) -> Result<bool> {
        let output = Self::list_containers(runtime, true, "{{.Names}}")?;
        Ok(output.lines().any(|line| line.trim() == container_name))
    }

    /// Return the names of all currently running containers.
    pub fn running_container_names(
        runtime: &ContainerRuntime,
    ) -> Result<std::collections::HashSet<String>> {
        Ok(Self::list_containers(runtime, false, "{{.Names}}")?
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Return the immutable image reference recorded on an existing container.
    pub fn container_image_reference(
        runtime: &ContainerRuntime,
        container_name: &str,
    ) -> Result<String> {
        let image = ContainerCommand::new(runtime)
            .subcommand("inspect")
            .arg("--format")
            .arg("{{.Config.Image}}")
            .arg(container_name)
            .execute_with_output()?
            .trim()
            .to_string();
        if image.is_empty() {
            return Err(VmError::Internal(format!(
                "Container '{container_name}' has no image reference"
            )));
        }
        Ok(image)
    }

    /// Copy files to/from a container.
    ///
    /// # Arguments
    /// * `source` - Source path (container:path or local path)
    /// * `destination` - Destination path (container:path or local path)
    pub fn copy(runtime: &ContainerRuntime, source: &str, destination: &str) -> Result<()> {
        ContainerCommand::new(runtime)
            .subcommand("cp")
            .arg(source)
            .arg(destination)
            .execute()
    }

    /// Start a container by name.
    pub fn start_container(runtime: &ContainerRuntime, container_name: &str) -> Result<()> {
        ContainerCommand::new(runtime)
            .subcommand("start")
            .arg(container_name)
            .execute_with_output()
            .map(|_| ())
    }

    /// Resume a paused container by name.
    pub fn unpause_container(runtime: &ContainerRuntime, container_name: &str) -> Result<()> {
        ContainerCommand::new(runtime)
            .subcommand("unpause")
            .arg(container_name)
            .execute_with_output()
            .map(|_| ())
    }

    /// Remove a container by name (with force flag).
    pub fn remove_container(
        runtime: &ContainerRuntime,
        container_name: &str,
        force: bool,
    ) -> Result<()> {
        let mut cmd = ContainerCommand::new(runtime).subcommand("rm");

        if force {
            cmd = cmd.arg("-f");
        }

        cmd.arg(container_name).execute()
    }

    /// Test container readiness by executing a simple command.
    pub fn test_container_readiness(runtime: &ContainerRuntime, container_name: &str) -> bool {
        ContainerCommand::new(runtime)
            .subcommand("exec")
            .arg(container_name)
            .arg("echo")
            .arg("ready")
            .execute()
            .is_ok()
    }

    /// Check if a Docker network exists by name.
    pub fn network_exists(runtime: &ContainerRuntime, network_name: &str) -> Result<bool> {
        let output = ContainerCommand::new(runtime)
            .subcommand("network")
            .arg("ls")
            .arg("--format")
            .arg("{{.Name}}")
            .execute_with_output()?;

        Ok(output.lines().any(|line| line.trim() == network_name))
    }

    /// Create a Docker network with the specified name.
    pub fn create_network(runtime: &ContainerRuntime, network_name: &str) -> Result<()> {
        vm_dbg!("Creating Docker network: {}", network_name);

        ContainerCommand::new(runtime)
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
    pub fn ensure_networks_exist(runtime: &ContainerRuntime, networks: &[String]) -> Result<()> {
        for network in networks {
            if !Self::network_exists(runtime, network)? {
                vm_dbg!("Network '{}' does not exist, creating it...", network);
                Self::create_network(runtime, network)?;
            } else {
                vm_dbg!("Network '{}' already exists", network);
            }
        }
        Ok(())
    }

    /// Build a custom Docker image from a Dockerfile.
    ///
    /// # Arguments
    /// * `runtime` - The validated container runtime
    /// * `dockerfile_path` - Path to the Dockerfile
    /// * `image_name` - Tag for the built image (e.g., "supercool:latest")
    /// * `context_dir` - Build context directory (usually parent of Dockerfile)
    /// * `build_args` - Optional build arguments to pass to docker build (--build-arg KEY=VALUE)
    pub fn build_custom_image(
        runtime: &ContainerRuntime,
        dockerfile_path: &std::path::Path,
        image_name: &str,
        context_dir: &std::path::Path,
        build_args: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<()> {
        use tracing::info;
        use vm_core::command_stream::stream_command;

        let executable = runtime.executable();

        info!(
            "Building custom base image '{}' from {:?} using {}...",
            image_name, dockerfile_path, executable
        );
        info!("This may take 5-15 minutes on first build...");

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
        stream_command(executable, &args)?;

        info!("Successfully built custom base image '{}'", image_name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ContainerEngine;

    #[test]
    fn test_docker_command_builder() {
        let runtime = ContainerRuntime::new(ContainerEngine::Docker);
        let cmd = ContainerCommand::new(&runtime)
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
        let runtime_path = temp.path().join("docker");
        let log = temp.path().join("args");
        fs::write(
            &runtime_path,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nprintf 'demo-dev\\ndemo-postgres\\ndemo-package-edge\\n'\n",
                log.display()
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&runtime_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&runtime_path, permissions).unwrap();

        let runtime = ContainerRuntime::with_executable(
            ContainerEngine::Docker,
            runtime_path.to_string_lossy(),
        );

        let services = ContainerOps::list_managed_service_containers(&runtime, "demo-dev").unwrap();

        assert_eq!(services, ["demo-postgres", "demo-package-edge"]);
        let args = fs::read_to_string(log).unwrap();
        assert!(args.contains("label=com.vm.managed=true"));
        assert!(args.contains("label=com.vm.instance=demo"));
    }
}
