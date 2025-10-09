//! Container lifecycle execution (start/stop/restart/kill)
use super::LifecycleOperations;
use crate::{
    audio::MacOSAudioManager,
    context::ProviderContext,
    docker::{build::BuildOperations, compose::ComposeOperations, DockerOps},
};
use vm_core::{
    command_stream::stream_command,
    error::{Result, VmError},
    vm_success,
};

impl<'a> LifecycleOperations<'a> {
    #[must_use = "container start results should be handled"]
    pub fn start_container(&self, container: Option<&str>) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;
        stream_command("docker", &["start", &target_container])
    }

    /// Start container with context-aware docker-compose regeneration
    #[must_use = "container start results should be handled"]
    pub fn start_container_with_context(
        &self,
        _container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        // Create worktrees directory if enabled (must exist before Docker mounts it)
        if self.config.worktrees.as_ref().is_some_and(|w| w.enabled)
            || context
                .global_config
                .as_ref()
                .is_some_and(|g| g.worktrees.enabled)
        {
            let compose_ops_temp =
                ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
            if let Some(worktrees_path) = compose_ops_temp.get_worktrees_host_path(context) {
                std::fs::create_dir_all(&worktrees_path).map_err(|e| {
                    VmError::Filesystem(format!(
                        "Failed to create worktrees directory at {}: {}. \
                         Check permissions for ~/.vm/worktrees/",
                        worktrees_path.display(),
                        e
                    ))
                })?;

                vm_success!("✓ Worktrees directory: {}", worktrees_path.display());
            }
        }

        // Regenerate docker-compose.yml with latest global config
        let build_ops = BuildOperations::new(self.config, self.temp_dir);
        let build_context = build_ops.prepare_build_context()?;

        let compose_ops = ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
        compose_ops.write_docker_compose(&build_context, context)?;

        // Use compose to start (handles both stopped containers and fresh starts)
        compose_ops.start_with_compose(context)
    }

    #[must_use = "container stop results should be handled"]
    pub fn stop_container(&self, container: Option<&str>) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;
        // Use a 1-second timeout for faster stops
        // Development VMs should respond quickly to SIGTERM
        // If they don't stop gracefully in 1 second, Docker will force kill
        // This is safe for dev environments where data persistence isn't critical
        duct::cmd("docker", &["stop", "-t", "1", &target_container])
            .run()
            .map_err(|e| {
                VmError::Internal(format!(
                    "Failed to stop container '{}': {}",
                    target_container, e
                ))
            })?;
        Ok(())
    }

    #[must_use = "container destruction results should be handled"]
    pub fn destroy_container(&self, container: Option<&str>) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;

        // Check if container exists before attempting destruction
        if !DockerOps::container_exists(&target_container).unwrap_or(false) {
            return Err(VmError::Internal(format!(
                "Container '{}' does not exist",
                target_container
            )));
        }

        let result = stream_command("docker", &["rm", "-f", &target_container]);

        // Only cleanup audio if it was enabled in the configuration
        if let Some(audio_service) = self.config.services.get("audio") {
            if audio_service.enabled {
                #[cfg(target_os = "macos")]
                if let Err(e) = MacOSAudioManager::cleanup() {
                    vm_warning!("Audio cleanup warning: {}", e);
                }
                #[cfg(not(target_os = "macos"))]
                MacOSAudioManager::cleanup();
            }
        }

        result
    }

    #[must_use = "container restart results should be handled"]
    pub fn restart_container(&self, container: Option<&str>) -> Result<()> {
        self.stop_container(container)?;
        self.start_container(container)
    }

    /// Restart container with context-aware docker-compose regeneration
    #[must_use = "container restart results should be handled"]
    pub fn restart_container_with_context(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        // Create worktrees directory if enabled (must exist before Docker mounts it)
        if self.config.worktrees.as_ref().is_some_and(|w| w.enabled)
            || context
                .global_config
                .as_ref()
                .is_some_and(|g| g.worktrees.enabled)
        {
            let compose_ops_temp =
                ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
            if let Some(worktrees_path) = compose_ops_temp.get_worktrees_host_path(context) {
                std::fs::create_dir_all(&worktrees_path).map_err(|e| {
                    VmError::Filesystem(format!(
                        "Failed to create worktrees directory at {}: {}. \
                         Check permissions for ~/.vm/worktrees/",
                        worktrees_path.display(),
                        e
                    ))
                })?;

                vm_success!("✓ Worktrees directory: {}", worktrees_path.display());
            }
        }

        // Regenerate docker-compose.yml with latest global config
        let build_ops = BuildOperations::new(self.config, self.temp_dir);
        let build_context = build_ops.prepare_build_context()?;

        let compose_ops = ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
        compose_ops.write_docker_compose(&build_context, context)?;

        // Stop the container first
        self.stop_container(container)?;

        // Use compose to start with updated configuration
        compose_ops.start_with_compose(context)
    }

    #[must_use = "container kill results should be handled"]
    pub fn kill_container(&self, container: Option<&str>) -> Result<()> {
        let container_name = self.container_name();
        let target_container = match container {
            None => container_name.clone(),
            Some(provided_name) => {
                // Try to resolve partial container names
                match self.resolve_container_name(provided_name) {
                    Ok(resolved_name) => resolved_name,
                    Err(_) => provided_name.to_string(), // Fall back to original name if resolution fails
                }
            }
        };
        stream_command("docker", &["kill", &target_container]).map_err(|e| {
            VmError::Internal(format!("Failed to kill container '{}'. Container may not exist or Docker may be unresponsive: {}", &target_container, e))
        })
    }
}