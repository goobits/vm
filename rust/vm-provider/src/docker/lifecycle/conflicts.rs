//! Existing-container and service-conflict handling.

use std::io::{self, IsTerminal, Write};

use tracing::{info, warn};

use super::LifecycleOperations;
use crate::{
    context::ProviderContext,
    docker::{ComposeCommand, DockerOps},
};
use vm_core::msg;
use vm_core::{
    command_stream::stream_command_visible,
    error::{Result, VmError},
    vm_hint, vm_progress, vm_warning,
};
use vm_messages::messages::MESSAGES;

impl<'a> LifecycleOperations<'a> {
    pub(super) fn start_orphaned_services_and_dev_container(
        &self,
        compose_path: &std::path::Path,
        container_name: &str,
    ) -> Result<()> {
        let expected_services =
            DockerOps::list_managed_service_containers(Some(self.executable), container_name)?;
        for service in &expected_services {
            if !DockerOps::container_exists(Some(self.executable), service).unwrap_or(false) {
                continue;
            }

            let running =
                DockerOps::is_container_running(Some(self.executable), service).unwrap_or(false);
            if running {
                continue;
            }

            if let Err(error) = DockerOps::start_container(Some(self.executable), service) {
                warn!("Failed to start existing service container '{service}': {error}");
            } else {
                info!("Started existing service: {service}");
            }
        }

        let flags = ["-d", "--no-deps", "--no-recreate", container_name];
        let args = ComposeCommand::build_args(compose_path, "up", &flags)?;
        let args: Vec<&str> = args.iter().map(String::as_str).collect();

        stream_command_visible(self.executable, &args).map_err(|error| {
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

    #[must_use = "existing container handling results should be checked"]
    pub(super) fn handle_existing_container(&self, context: &ProviderContext) -> Result<()> {
        let container_name = self.container_name();
        let is_running = DockerOps::is_container_running(Some(self.executable), &container_name)
            .map_err(|error| warn!("Failed to check running containers: {error}"))
            .unwrap_or(false);

        warn!(
            "Container '{}' already exists (status: {}).",
            container_name,
            if is_running { "running" } else { "stopped" }
        );

        if !io::stdin().is_terminal() {
            return Err(VmError::Internal(format!(
                "Container '{container_name}' already exists. In non-interactive mode, use:\n\
                 - 'vm start' to start the existing container\n\
                 - 'vm remove --force' followed by 'vm run linux' to recreate it"
            )));
        }

        self.prompt_for_existing_container(
            if is_running {
                MESSAGES.service.docker_container_exists_running
            } else {
                MESSAGES.service.docker_container_exists_stopped
            },
            || {
                if is_running {
                    info!("Using existing running container.");
                    Ok(())
                } else {
                    info!("{}", MESSAGES.service.docker_container_starting);
                    self.start_container(None, context)
                }
            },
            || {
                info!("{}", MESSAGES.service.docker_container_recreating);
                self.destroy_container(None, context)?;
                self.create_container(context)
            },
        )
    }

    #[must_use = "existing container handling results should be checked"]
    pub(super) fn handle_existing_container_with_instance(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        let container_name = self.container_name_with_instance(instance_name);
        let is_running = DockerOps::is_container_running(Some(self.executable), &container_name)
            .map_err(|error| warn!("Failed to check running containers: {error}"))
            .unwrap_or(false);

        warn!(
            "Container '{}' already exists (status: {}).",
            container_name,
            if is_running { "running" } else { "stopped" }
        );

        if !io::stdin().is_terminal() {
            return Err(VmError::Internal(format!(
                "Container '{container_name}' already exists. In non-interactive mode, use:\n\
                 - 'vm start {instance_name}' to start the existing container\n\
                 - 'vm remove {instance_name} --force' followed by 'vm run linux as {instance_name}' to recreate it"
            )));
        }

        self.prompt_for_existing_container(
            if is_running {
                MESSAGES.service.docker_container_exists_running
            } else {
                MESSAGES.service.docker_container_exists_stopped
            },
            || {
                if is_running {
                    info!("Using existing running container.");
                    Ok(())
                } else {
                    info!("{}", MESSAGES.service.docker_container_starting);
                    self.start_container(Some(&container_name), context)
                }
            },
            || {
                info!("{}", MESSAGES.service.docker_container_recreating);
                self.destroy_container(Some(&container_name), context)?;
                self.create_container_with_instance(instance_name, context)
            },
        )
    }

    fn prompt_for_existing_container(
        &self,
        first_option: &str,
        reuse: impl FnOnce() -> Result<()>,
        recreate: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        vm_progress!(
            "{}",
            msg!(
                MESSAGES.service.docker_container_exists_prompt,
                option1 = first_option
            )
        );
        print!("{}", MESSAGES.service.docker_container_choice_prompt);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        match input.trim() {
            "1" => reuse(),
            "2" => recreate(),
            _ => {
                vm_progress!("Operation cancelled.");
                Ok(())
            }
        }
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
        let orphaned =
            DockerOps::list_managed_service_containers(Some(self.executable), &environment)?;

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
        vm_warning!(
            "Reusing existing service containers to preserve data: {}",
            orphaned.join(", ")
        );
        vm_hint!("Remove them only through the approved VM maintenance workflow");
        Ok(true)
    }
}
