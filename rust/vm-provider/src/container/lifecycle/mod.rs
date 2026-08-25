//! Docker-compatible container lifecycle management operations.

// Module declarations in dependency order
mod configuration;
mod conflicts;
pub mod creation;
mod diagnostics;
pub mod execution;
pub mod health;
pub mod helpers;
pub mod interaction;
mod package_edge;
mod pipx;
pub mod provisioning;
pub mod status;

use crate::{TempProvider, TempVmState};
use tracing::{info, warn};
use vm_config::config::VmConfig;
use vm_core::{
    command_stream::stream_command,
    error::{Result, VmError},
};

use super::{
    artifacts::compose_path, compose::ComposeOperations, engine::ComposeRuntime, mountpoints,
    ContainerEngine,
};

// Constants for container lifecycle operations
const DEFAULT_SHELL: &str = "zsh";
const CONTAINER_READINESS_MAX_ATTEMPTS: u32 = 30;
const CONTAINER_READINESS_SLEEP_SECONDS: u64 = 2;
const ANSIBLE_PLAYBOOK_PATH: &str = "/app/shared/ansible/playbook.yml";
const TEMP_CONFIG_PATH: &str = "/tmp/vm-config.json";

/// Main lifecycle operations struct
pub struct LifecycleOperations<'a> {
    pub config: &'a VmConfig,
    pub generated_dir: &'a std::path::PathBuf,
    pub project_dir: &'a std::path::PathBuf,
    pub executable: &'a str,
    pub(crate) compose_runtime: ComposeRuntime,
}

impl<'a> LifecycleOperations<'a> {
    #[cfg(test)]
    fn new(
        config: &'a VmConfig,
        generated_dir: &'a std::path::PathBuf,
        project_dir: &'a std::path::PathBuf,
        executable: &'a str,
    ) -> Self {
        Self {
            config,
            generated_dir,
            project_dir,
            executable,
            compose_runtime: ComposeRuntime::BuiltIn,
        }
    }

    pub fn with_engine(
        config: &'a VmConfig,
        generated_dir: &'a std::path::PathBuf,
        project_dir: &'a std::path::PathBuf,
        executable: &'a str,
        engine: ContainerEngine,
    ) -> Self {
        Self {
            config,
            generated_dir,
            project_dir,
            executable,
            compose_runtime: engine.compose_runtime(),
        }
    }
}

// TempProvider trait implementation (delegates to creation/execution modules)
impl<'a> TempProvider for LifecycleOperations<'a> {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        info!("Updating container mounts");

        if self.is_container_running(&state.container_name)? {
            info!("Stopping container before mount update");
            stream_command(self.executable, &["stop", &state.container_name])?;
        }

        info!("Recreating container with new mounts");
        self.recreate_with_mounts(state)?;

        info!("Starting container after mount update");
        let compose_path = compose_path(self.generated_dir, None);
        self.compose_runtime
            .command(self.executable, &compose_path, "up", &["-d"])?
            .stream()?;

        info!("Checking container health after mount update");
        if !self.check_container_health(&state.container_name)? {
            return Err(VmError::Internal(format!(
                "Container '{}' is not healthy after mount update. Check container logs for issues",
                state.container_name
            )));
        }

        info!("Container mounts updated successfully");
        Ok(())
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        info!("Generating updated Compose configuration");

        let temp_config = self.prepare_temp_config()?;
        mountpoints::prepare(&temp_config, self.project_dir, Some(&state.mounts))?;
        let compose_ops = ComposeOperations::new(
            &temp_config,
            self.generated_dir,
            self.project_dir,
            self.executable,
        );
        let content = compose_ops.render_docker_compose_with_mounts(state)?;
        let compose_path = compose_path(self.generated_dir, None);
        // Atomic write so a crash mid-write can't leave a half-rendered compose
        // file that the next `docker-compose up` would fail to parse.
        crate::container::artifacts::secure_write_if_changed(&compose_path, content.as_bytes())?;

        info!("Removing old container before applying mount configuration");
        if let Err(e) = stream_command(self.executable, &["rm", "-f", &state.container_name]) {
            warn!(
                "Failed to remove old container {}: {}",
                &state.container_name, e
            );
        }

        info!("Container mount configuration updated");
        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        for _ in 0..CONTAINER_READINESS_MAX_ATTEMPTS {
            if stream_command(self.executable, &["exec", container_name, "echo", "ready"]).is_ok() {
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_secs(
                CONTAINER_READINESS_SLEEP_SECONDS,
            ));
        }
        Ok(false)
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        let output = std::process::Command::new(self.executable)
            .args([
                "inspect",
                "--type",
                "container",
                "--format",
                "{{.State.Status}}",
                container_name,
            ])
            .output()?;
        if !output.status.success() {
            return Ok(false);
        }
        let output_str = String::from_utf8_lossy(&output.stdout);
        let status = output_str.trim();
        Ok(status == "running")
    }
}
