//! Existing-container and service-conflict handling.

use tracing::{info, warn};

use super::LifecycleOperations;
use crate::{container::ContainerOps, context::ProviderContext};
use vm_core::error::{Result, VmError};

impl<'a> LifecycleOperations<'a> {
    pub(super) fn start_orphaned_services_and_dev_container(
        &self,
        compose_path: &std::path::Path,
        container_name: &str,
    ) -> Result<()> {
        let expected_services =
            ContainerOps::list_managed_service_containers(&self.runtime, container_name)?;
        let running = ContainerOps::running_container_names(&self.runtime).unwrap_or_default();
        for service in &expected_services {
            if running.contains(service) {
                continue;
            }

            if let Err(error) = ContainerOps::start_container(&self.runtime, service) {
                warn!("Failed to start existing service container '{service}': {error}");
            } else {
                info!("Started existing service: {service}");
            }
        }

        let flags = ["-d", "--no-deps", "--no-recreate", container_name];
        self.runtime
            .compose_invocation(compose_path, "up", &flags)?
            .stream()
            .map_err(|error| {
                VmError::Internal(format!(
                "Failed to start dev container '{container_name}' while reusing services: {error}"
            ))
            })
    }

    pub(super) fn handle_compose_start_error(
        &self,
        error: VmError,
        error_message: String,
        instance_name: Option<&str>,
    ) -> VmError {
        if !error_message.contains("is already in use") && !error_message.contains("Conflict") {
            return error;
        }

        let recovery = if let Some(name) = instance_name {
            format!("vm remove {name} --force, then vm run linux as {name}")
        } else {
            "vm remove --force, then vm run linux".to_string()
        };

        VmError::Internal(format!(
            "Container name conflict: {error_message}. A previous creation may have stopped partway through; recover with `{recovery}`"
        ))
    }

    pub(super) fn check_for_orphaned_containers(
        &self,
        instance_name: Option<&str>,
        context: &ProviderContext,
    ) -> Result<bool> {
        let environment = instance_name.map_or_else(
            || self.container_name(),
            |instance| self.container_name_with_instance(instance),
        );
        let orphaned = ContainerOps::list_managed_service_containers(&self.runtime, &environment)?;

        if orphaned.is_empty() {
            return Ok(false);
        }
        if !context.preserve_services {
            return Err(VmError::Provider(format!(
                "Found existing service containers that would conflict: {}. Preserve them or remove them through the approved VM maintenance workflow.",
                orphaned.join(", ")
            )));
        }

        warn!(
            "Found existing service containers; continuing to reuse them: {}",
            orphaned.join(", ")
        );
        warn!(
            "Reusing existing service containers to preserve data: {}",
            orphaned.join(", ")
        );
        info!("Remove them only through the approved VM maintenance workflow");
        Ok(true)
    }
}
