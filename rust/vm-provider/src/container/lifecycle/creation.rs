//! Container creation orchestration.

use std::fs;

use tracing::{info, warn};

use super::LifecycleOperations;
use crate::{
    audio::MacOSAudioManager,
    container::{build::BuildOperations, compose::ComposeOperations, mountpoints, ContainerOps},
    context::ProviderContext,
};
use vm_core::{
    error::{Result, VmError},
    vm_dbg,
};

impl<'a> LifecycleOperations<'a> {
    #[must_use = "container creation results should be handled"]
    pub fn create_container(&self, context: &ProviderContext) -> Result<()> {
        self.create_container_impl(None, context)
    }

    fn create_container_impl(
        &self,
        instance_name: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        if let Some(vm_config) = &self.config.vm {
            self.check_memory_allocation(vm_config);
        }
        self.check_docker_build_requirements();

        #[cfg(target_os = "windows")]
        {
            let worktrees_enabled = self
                .config
                .host_sync
                .as_ref()
                .and_then(|sync| sync.worktrees.as_ref())
                .is_some_and(|worktrees| worktrees.enabled)
                || context
                    .global_config
                    .as_ref()
                    .is_some_and(|config| config.worktrees.enabled);
            if worktrees_enabled {
                let is_wsl = std::path::Path::new("/proc/version").exists()
                    && std::fs::read_to_string("/proc/version")
                        .ok()
                        .is_some_and(|version| version.to_lowercase().contains("microsoft"));
                if !is_wsl {
                    return Err(VmError::Config(
                        "Git worktrees require WSL2 on Windows.\n\
                         Native Windows paths (C:\\) cannot be translated to Linux container paths.\n\
                         \n\
                         Solutions:\n\
                         1. Install WSL2: https://aka.ms/wsl2 (recommended)\n\
                         2. Disable worktrees: vm config set worktrees.enabled false\n\
                         \n\
                         Note: Windows native support planned for future release (Git 2.48+)."
                            .into(),
                    ));
                }
                info!("WSL2 detected; worktrees are supported");
            }
        }

        let container_name = instance_name.map_or_else(
            || self.container_name(),
            |name| self.container_name_with_instance(name),
        );
        let container_exists = ContainerOps::container_exists(&self.runtime, &container_name)
            .map_err(|error| warn!("Failed to check existing containers: {error}"))
            .unwrap_or(false);
        if container_exists {
            return Err(VmError::Conflict(format!(
                "Container '{container_name}' already exists; start it or remove it before creating a replacement"
            )));
        }

        if self
            .config
            .services
            .get("audio")
            .is_some_and(|service| service.enabled)
        {
            #[cfg(target_os = "macos")]
            if let Err(error) = MacOSAudioManager::setup() {
                warn!("Audio setup failed: {error}");
            }
            #[cfg(not(target_os = "macos"))]
            MacOSAudioManager::setup();
        }

        let modified_config = self.prepare_config_for_build()?;
        mountpoints::prepare(&modified_config, self.project_dir, None)?;
        let build_ops = BuildOperations::with_runtime(
            &modified_config,
            self.generated_dir,
            self.runtime.clone(),
        );
        let (build_context, base_image, is_snapshot, prepared_base_identity) =
            build_ops.prepare_build_context()?;

        if let Some(networking) = &modified_config.networking {
            if !networking.networks.is_empty() {
                info!("Ensuring Docker networks exist: {:?}", networking.networks);
                ContainerOps::ensure_networks_exist(&self.runtime, &networking.networks)?;
            }
        }

        let compose_ops = ComposeOperations::with_runtime(
            &modified_config,
            self.generated_dir,
            self.project_dir,
            self.runtime.clone(),
        );
        let build_args = build_ops.gather_build_args(&base_image);
        let base_image_identity = match prepared_base_identity {
            Some(identity) => identity,
            None => build_ops.image_identity(&base_image)?,
        };
        let image_tag = build_ops.derived_image_tag_with_args(
            &base_image,
            &base_image_identity,
            &build_context,
            &build_args,
        )?;
        let compose_path = match instance_name {
            Some(name) => compose_ops.write_docker_compose_with_instance_and_image_tag(
                &build_context,
                name,
                context,
                &image_tag,
            )?,
            None => compose_ops.write_docker_compose_with_image_tag(
                &build_context,
                context,
                &image_tag,
            )?,
        };

        let has_orphaned_services = self.check_for_orphaned_containers(instance_name, context)?;
        if build_ops.image_exists(&image_tag)? {
            vm_dbg!("Reusing cached derived image '{}'", image_tag);
        } else {
            let mut command = self
                .runtime
                .compose_invocation(&compose_path, "build", &[])?;
            command.extend(build_args.iter().map(String::as_str));

            vm_dbg!(
                "Building derived image '{}' with {} extra arguments",
                image_tag,
                build_args.len()
            );
            vm_dbg!("Build context directory: {}", build_context.display());
            if let Ok(entries) = fs::read_dir(&build_context) {
                vm_dbg!(
                    "Build context contains {} files/directories",
                    entries.count()
                );
            }

            command.stream().map_err(|error| match instance_name {
                Some(name) => VmError::Internal(format!(
                    "Docker build failed for project '{}' instance '{}'. Check that Docker is running and build context is valid: {}",
                    self.project_name(),
                    name,
                    error
                )),
                None => VmError::Internal(format!(
                    "Docker build failed for project '{}'. Check that Docker is running and build context is valid: {}",
                    self.project_name(),
                    error
                )),
            })?;
        }

        if has_orphaned_services {
            self.start_orphaned_services_and_dev_container(&compose_path, &container_name)?;
        } else {
            self.runtime
                .compose_invocation(&compose_path, "up", &["-d"])?
                .stream()
                .map_err(|error| {
                    let error_message = error.to_string();
                    self.handle_compose_start_error(error, error_message, instance_name)
                })?;
        }

        let provision_context = context.clone().with_snapshot(is_snapshot);
        match instance_name {
            Some(name) => {
                self.provision_container_with_instance_and_context(name, &provision_context)
            }
            None => self.provision_container_with_context(&provision_context),
        }
    }

    #[must_use = "container creation results should be handled"]
    pub fn create_container_with_instance(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        self.create_container_impl(Some(instance_name), context)
    }
}
