//! Container lifecycle execution (start/stop/restart/kill)
use super::LifecycleOperations;
use crate::{
    audio::MacOSAudioManager,
    context::ProviderContext,
    docker::{compose::ComposeOperations, DockerOps},
};
use tracing::{info, warn};
use vm_core::{
    command_stream::stream_command,
    error::{Result, VmError},
};

use vm_core::vm_warning;

impl<'a> LifecycleOperations<'a> {
    #[must_use = "container start results should be handled"]
    pub fn start_container(&self, container: Option<&str>) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;
        stream_command(self.executable, &["start", &target_container])
    }

    /// Start container with context-aware docker-compose regeneration
    #[must_use = "container start results should be handled"]
    pub fn start_container_with_context(
        &self,
        _container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        let compose_ops = self.regenerate_compose_with_context(context)?;

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
        duct::cmd(self.executable, &["stop", "-t", "1", &target_container])
            .run()
            .map_err(|e| {
                VmError::Internal(format!(
                    "Failed to stop container '{target_container}': {e}"
                ))
            })?;
        Ok(())
    }

    #[must_use = "container destruction results should be handled"]
    pub fn destroy_container(&self, container: Option<&str>) -> Result<()> {
        self.destroy_container_with_context(container, &ProviderContext::default())
    }

    pub fn destroy_container_with_context(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;

        // Check if container exists before attempting destruction
        if !DockerOps::container_exists(Some(self.executable), &target_container).unwrap_or(false) {
            return Err(VmError::Internal(format!(
                "Container '{target_container}' does not exist"
            )));
        }

        // Remove the main dev container
        let result = stream_command(self.executable, &["rm", "-f", &target_container]);

        // Optionally remove service containers based on context
        if !context.preserve_services {
            info!("Removing service containers (--preserve-services=false)");
            let compose_ops = ComposeOperations::new(
                self.config,
                self.temp_dir,
                self.project_dir,
                self.executable,
            );
            let expected_services = compose_ops.get_expected_service_containers();

            for service_name in expected_services {
                let exists = DockerOps::container_exists(Some(self.executable), &service_name)
                    .unwrap_or(false);
                if !exists {
                    continue;
                }

                info!("Removing service container: {}", service_name);
                if let Err(e) =
                    DockerOps::remove_container(Some(self.executable), &service_name, true)
                {
                    warn!("Failed to remove service container {}: {}", service_name, e);
                }
            }
        } else {
            info!("Preserving service containers (--preserve-services=true)");
        }

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

        // Clean up the temporary instance directory to prevent disk leaks
        if self.temp_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(self.temp_dir) {
                vm_warning!(
                    "Failed to remove temp directory {}: {}",
                    self.temp_dir.display(),
                    e
                );
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
        let compose_ops = self.regenerate_compose_with_context(context)?;

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
        stream_command(self.executable, &["kill", &target_container]).map_err(|e| {
            VmError::Internal(format!("Failed to kill container '{}'. Container may not exist or Docker may be unresponsive: {}", &target_container, e))
        })
    }
}
