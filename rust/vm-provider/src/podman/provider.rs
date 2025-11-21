//! Podman provider implementation.
//!
//! This module implements the Provider trait for Podman by wrapping
//! the DockerProvider and intercepting command execution to use Podman instead.

use std::path::Path;
use vm_core::command_stream::is_tool_installed;
use vm_core::error::{Result, VmError};

use crate::{
    context::ProviderContext, docker::DockerProvider, InstanceInfo, Provider, SnapshotRequest,
    SnapshotRestoreRequest, TempProvider, VmStatusReport,
};
use vm_config::config::VmConfig;

use super::command::detect_compose_command;

/// Podman provider wrapping Docker provider implementation.
///
/// This provider reuses all the Docker provider logic (templates, build operations,
/// lifecycle management) but executes commands using `podman` instead of `docker`.
#[derive(Clone)]
pub struct PodmanProvider {
    /// The underlying Docker provider (used for all logic except command execution)
    docker_provider: DockerProvider,
}

impl PodmanProvider {
    /// Create a new Podman provider instance.
    pub fn new(config: VmConfig) -> Result<Self> {
        // Check if podman is installed
        if !is_tool_installed("podman") {
            return Err(VmError::Dependency("Podman".into()));
        }

        // Validate podman environment
        validate_podman_environment()?;

        // Detect compose command availability
        detect_compose_command()?;

        // Create a modified config for the Docker provider
        // The Docker provider will generate all the templates and logic,
        // we just need to ensure it uses the right configuration
        let mut docker_config = config.clone();

        // Keep the provider as "podman" in our config, but the docker_provider
        // will use its own logic internally
        docker_config.provider = Some("docker".to_string());

        // Create the underlying Docker provider
        let docker_provider = DockerProvider::new(docker_config, Some("podman".to_string()))?;

        Ok(Self { docker_provider })
    }
}

/// Validate that Podman is installed and working.
fn validate_podman_environment() -> Result<()> {
    use std::process::Command;

    // Check 1: Podman installed
    if !Command::new("podman").arg("--version").status()?.success() {
        return Err(VmError::Dependency(
            "Podman is not installed. Install from: https://podman.io/getting-started/installation"
                .to_string(),
        ));
    }

    // Check 2: Podman working (can list containers)
    let output = Command::new("podman").arg("ps").output()?;
    if !output.status.success() {
        if String::from_utf8_lossy(&output.stderr).contains("permission denied") {
            return Err(VmError::Provider(
                "Podman permission denied. Ensure you can run: podman ps".to_string(),
            ));
        } else {
            return Err(VmError::Provider(format!(
                "Podman is not working correctly: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
    }

    Ok(())
}

impl Provider for PodmanProvider {
    fn name(&self) -> &'static str {
        "podman"
    }

    fn create(&self) -> Result<()> {
        self.create_with_context(&ProviderContext::default())
    }

    fn create_with_context(&self, context: &ProviderContext) -> Result<()> {
        // Delegate to Docker provider - it will generate compose files and templates
        // The actual command execution will be intercepted by our lifecycle wrapper
        self.docker_provider.create_with_context(context)
    }

    fn create_instance(&self, instance_name: &str) -> Result<()> {
        self.docker_provider.create_instance(instance_name)
    }

    fn create_instance_with_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        self.docker_provider
            .create_instance_with_context(instance_name, context)
    }

    fn start(&self, container: Option<&str>) -> Result<()> {
        self.docker_provider.start(container)
    }

    fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        self.docker_provider.start_with_context(container, context)
    }

    fn stop(&self, container: Option<&str>) -> Result<()> {
        self.docker_provider.stop(container)
    }

    fn destroy(&self, container: Option<&str>) -> Result<()> {
        self.docker_provider.destroy(container)
    }

    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        self.docker_provider.ssh(container, relative_path)
    }

    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        self.docker_provider.exec(container, cmd)
    }

    fn logs(&self, container: Option<&str>) -> Result<()> {
        self.docker_provider.logs(container)
    }

    fn logs_extended(
        &self,
        container: Option<&str>,
        follow: bool,
        tail: usize,
        service: Option<&str>,
        config: &VmConfig,
    ) -> Result<()> {
        self.docker_provider
            .logs_extended(container, follow, tail, service, config)
    }

    fn copy(&self, source: &str, destination: &str, container: Option<&str>) -> Result<()> {
        self.docker_provider.copy(source, destination, container)
    }

    fn get_container_mounts(&self, container_name: &str) -> Result<Vec<String>> {
        self.docker_provider.get_container_mounts(container_name)
    }

    fn status(&self, container: Option<&str>) -> Result<()> {
        self.docker_provider.status(container)
    }

    fn restart(&self, container: Option<&str>) -> Result<()> {
        self.docker_provider.restart(container)
    }

    fn restart_with_context(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        self.docker_provider
            .restart_with_context(container, context)
    }

    fn provision(&self, container: Option<&str>) -> Result<()> {
        self.docker_provider.provision(container)
    }

    fn list(&self) -> Result<()> {
        self.docker_provider.list()
    }

    fn kill(&self, container: Option<&str>) -> Result<()> {
        self.docker_provider.kill(container)
    }

    fn get_status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
        // Get the report from docker provider and update the provider name
        let mut report = self.docker_provider.get_status_report(container)?;
        report.provider = "podman".to_string();
        Ok(report)
    }

    fn get_sync_directory(&self) -> String {
        self.docker_provider.get_sync_directory()
    }

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        // Delegate to docker provider's temp implementation
        Some(self)
    }

    fn supports_multi_instance(&self) -> bool {
        self.docker_provider.supports_multi_instance()
    }

    fn resolve_instance_name(&self, instance: Option<&str>) -> Result<String> {
        self.docker_provider.resolve_instance_name(instance)
    }

    fn list_instances(&self) -> Result<Vec<InstanceInfo>> {
        self.docker_provider.list_instances()
    }

    fn snapshot(&self, request: &SnapshotRequest) -> Result<()> {
        self.docker_provider.snapshot(request)
    }

    fn restore_snapshot(&self, request: &SnapshotRestoreRequest) -> Result<()> {
        self.docker_provider.restore_snapshot(request)
    }

    fn clone_box(&self) -> Box<dyn Provider> {
        Box::new(self.clone())
    }
}

impl TempProvider for PodmanProvider {
    fn update_mounts(&self, state: &crate::TempVmState) -> Result<()> {
        // Delegate to docker provider
        self.docker_provider
            .as_temp_provider()
            .ok_or_else(|| {
                VmError::Internal("Docker provider does not support temp operations".to_string())
            })?
            .update_mounts(state)
    }

    fn recreate_with_mounts(&self, state: &crate::TempVmState) -> Result<()> {
        self.docker_provider
            .as_temp_provider()
            .ok_or_else(|| {
                VmError::Internal("Docker provider does not support temp operations".to_string())
            })?
            .recreate_with_mounts(state)
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        self.docker_provider
            .as_temp_provider()
            .ok_or_else(|| {
                VmError::Internal("Docker provider does not support temp operations".to_string())
            })?
            .check_container_health(container_name)
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        self.docker_provider
            .as_temp_provider()
            .ok_or_else(|| {
                VmError::Internal("Docker provider does not support temp operations".to_string())
            })?
            .is_container_running(container_name)
    }
}
