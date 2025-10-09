//! Docker container lifecycle management operations.

// Module declarations in dependency order
pub mod creation;
pub mod execution;
pub mod health;
pub mod helpers;
pub mod interaction;
pub mod packages;
pub mod provisioning;
pub mod status;

use crate::{
    progress::ProgressReporter,
    TempProvider, TempVmState,
};
use std::fs;
use vm_config::config::VmConfig;
use vm_core::{command_stream::stream_command, error::{Result, VmError}};

use super::{compose::ComposeOperations, ComposeCommand};

// Constants for container lifecycle operations
const DEFAULT_SHELL: &str = "zsh";
const CONTAINER_READINESS_MAX_ATTEMPTS: u32 = 30;
const CONTAINER_READINESS_SLEEP_SECONDS: u64 = 2;
const ANSIBLE_PLAYBOOK_PATH: &str = "/app/shared/ansible/playbook.yml";
const TEMP_CONFIG_PATH: &str = "/tmp/vm-config.json";

/// Main lifecycle operations struct
pub struct LifecycleOperations<'a> {
    pub config: &'a VmConfig,
    pub temp_dir: &'a std::path::PathBuf,
    pub project_dir: &'a std::path::PathBuf,
}

impl<'a> LifecycleOperations<'a> {
    /// Constructor - only public method in mod.rs
    pub fn new(
        config: &'a VmConfig,
        temp_dir: &'a std::path::PathBuf,
        project_dir: &'a std::path::PathBuf,
    ) -> Self {
        Self {
            config,
            temp_dir,
            project_dir,
        }
    }
}

// TempProvider trait implementation (delegates to creation/execution modules)
impl<'a> TempProvider for LifecycleOperations<'a> {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        let progress = ProgressReporter::new();
        let main_phase = progress.start_phase("Updating container mounts");
        ProgressReporter::task(&main_phase, "Checking container status...");

        if self.is_container_running(&state.container_name)? {
            ProgressReporter::task(&main_phase, "Stopping container...");
            stream_command("docker", &["stop", &state.container_name])?;
        }

        ProgressReporter::task(&main_phase, "Recreating container with new mounts...");
        self.recreate_with_mounts(state)?;

        ProgressReporter::task(&main_phase, "Starting container...");
        let compose_path = self.temp_dir.join("docker-compose.yml");
        let args = ComposeCommand::build_args(&compose_path, "up", &["-d"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command("docker", &args_refs)?;

        ProgressReporter::task(&main_phase, "Checking container health...");
        if !self.check_container_health(&state.container_name)? {
            ProgressReporter::finish_phase(
                &main_phase,
                "Mount update failed - container not healthy",
            );
            return Err(VmError::Internal(format!(
                "Container '{}' is not healthy after mount update. Check container logs for issues",
                &state.container_name
            )));
        }

        ProgressReporter::finish_phase(&main_phase, "Mounts updated successfully");
        Ok(())
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        let progress = ProgressReporter::new();
        let phase = progress.start_phase("Recreating container configuration");
        ProgressReporter::task(&phase, "Generating updated docker-compose.yml...");

        let temp_config = self.prepare_temp_config()?;
        let compose_ops = ComposeOperations::new(&temp_config, self.temp_dir, self.project_dir);
        let content = compose_ops.render_docker_compose_with_mounts(state)?;
        let compose_path = self.temp_dir.join("docker-compose.yml");
        fs::write(&compose_path, content.as_bytes())?;

        ProgressReporter::task(&phase, "Removing old container...");
        if let Err(e) = stream_command("docker", &["rm", "-f", &state.container_name]) {
            eprintln!(
                "Warning: Failed to remove old container {}: {}",
                &state.container_name, e
            );
        }

        ProgressReporter::finish_phase(&phase, "Container configuration updated");
        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        for _ in 0..CONTAINER_READINESS_MAX_ATTEMPTS {
            if stream_command("docker", &["exec", container_name, "echo", "ready"]).is_ok() {
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_secs(
                CONTAINER_READINESS_SLEEP_SECONDS,
            ));
        }
        Ok(false)
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        let output = std::process::Command::new("docker")
            .args(["inspect", "--format", "{{.State.Status}}", container_name])
            .output()?;
        if !output.status.success() {
            return Ok(false);
        }
        let output_str = String::from_utf8_lossy(&output.stdout);
        let status = output_str.trim();
        Ok(status == "running")
    }
}