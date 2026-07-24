//! Existing-container and service-conflict handling.

use std::io::{self, IsTerminal, Write};

use tracing::{error, info, warn};

use super::LifecycleOperations;
use crate::{
    context::ProviderContext,
    docker::{compose::ComposeOperations, ComposeCommand, DockerOps},
};
use vm_core::msg;
use vm_core::{
    command_stream::stream_command_visible,
    error::{Result, VmError},
};
use vm_messages::messages::MESSAGES;

impl<'a> LifecycleOperations<'a> {
    pub(super) fn start_orphaned_services_and_dev_container(
        &self,
        compose_ops: &ComposeOperations,
        compose_path: &std::path::Path,
        container_name: &str,
    ) -> Result<()> {
        let instance = compose_ops.instance_name_from_container(container_name);
        let expected_services = compose_ops.get_expected_service_containers(instance.as_deref());
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

        eprintln!("\n⚠️  Container name conflict detected");
        eprintln!("\n   Docker error:");
        eprintln!("   {error_message}");
        eprintln!("\n   This usually means a previous creation failed partway through.");
        self.list_project_containers_for_user();
        eprintln!("\n💡 Recommended fix:");
        if let Some(name) = instance_name {
            eprintln!("      vm remove {name} --force");
            eprintln!("      vm create {name}");
        } else {
            eprintln!("      vm remove --force");
            eprintln!("      vm create");
        }
        eprintln!();

        VmError::Internal(format!(
            "Container name conflict detected. See error details above.\n\nOriginal error: {error_message}"
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
                 - 'vm remove --force' followed by 'vm create' to recreate it"
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
                 - 'vm remove {instance_name} --force' followed by 'vm create {instance_name}' to recreate it"
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
        info!(
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
                error!("Operation cancelled.");
                Ok(())
            }
        }
    }

    fn list_project_containers_for_user(&self) {
        let Ok(containers) = DockerOps::list_containers(Some(self.executable), true, "{{.Names}}")
        else {
            return;
        };
        let project_prefix = format!("{}-", self.project_name());
        let project_containers: Vec<&str> = containers
            .lines()
            .map(str::trim)
            .filter(|name| name.starts_with(&project_prefix))
            .collect();

        if !project_containers.is_empty() {
            eprintln!("\n   Existing containers for {}:", self.project_name());
            for container in project_containers {
                eprintln!("   • {container}");
            }
        }
    }

    pub(super) fn check_for_orphaned_containers(
        &self,
        instance_name: Option<&str>,
        context: &ProviderContext,
    ) -> Result<bool> {
        let all_containers = DockerOps::list_containers(Some(self.executable), true, "{{.Names}}")?;
        let compose_ops = ComposeOperations::new(
            self.config,
            self.generated_dir,
            self.project_dir,
            self.executable,
        );
        let service_patterns = compose_ops.get_expected_service_containers(instance_name);
        let orphaned: Vec<&str> = all_containers
            .lines()
            .map(str::trim)
            .filter(|container| service_patterns.iter().any(|pattern| container == pattern))
            .collect();

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
        eprintln!("\n⚠️  Existing service containers detected (will reuse data):\n");
        for container in orphaned {
            eprintln!("   • {container}");
        }
        eprintln!("\n💡 We'll reuse these to preserve your data.");
        eprintln!("   Remove them only through the approved VM maintenance workflow.\n");
        Ok(true)
    }
}
