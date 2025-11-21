//! Container creation and setup
use std::borrow::Cow;
use std::fs;
use std::io::IsTerminal;
use std::io::{self, Write};
use tracing::{error, info, warn};

use super::LifecycleOperations;
use crate::{
    audio::MacOSAudioManager,
    context::ProviderContext,
    docker::{build::BuildOperations, compose::ComposeOperations, ComposeCommand, DockerOps},
};
use vm_cli::msg;
use vm_config::config::VmConfig;
use vm_core::{
    command_stream::stream_command_visible,
    error::{Result, VmError},
    vm_dbg,
};
use vm_messages::messages::MESSAGES;

impl<'a> LifecycleOperations<'a> {
    #[must_use = "container creation results should be handled"]
    pub fn create_container(&self) -> Result<()> {
        self.create_container_with_context(&ProviderContext::default())
    }

    #[must_use = "container creation results should be handled"]
    pub fn create_container_with_context(&self, context: &ProviderContext) -> Result<()> {
        self.create_container_impl(None, context)
    }

    /// Unified container creation implementation for both default and instance paths.
    /// This eliminates 165 lines of duplication between the two code paths.
    fn create_container_impl(
        &self,
        instance_name: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        self.check_daemon_is_running()?;
        self.handle_potential_issues();
        self.check_docker_build_requirements();

        // Check platform support for worktrees (Windows native not supported)
        #[cfg(target_os = "windows")]
        {
            if self
                .config
                .host_sync
                .as_ref()
                .and_then(|hs| hs.worktrees.as_ref())
                .is_some_and(|w| w.enabled)
                || context
                    .global_config
                    .as_ref()
                    .is_some_and(|g| g.worktrees.enabled)
            {
                // Check if running in WSL
                let is_wsl = std::path::Path::new("/proc/version").exists()
                    && std::fs::read_to_string("/proc/version")
                        .ok()
                        .map_or(false, |v| v.to_lowercase().contains("microsoft"));

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

                info!("✓ WSL2 detected - worktrees will work correctly");
            }
        }

        // Check if container already exists
        let container_name = match instance_name {
            Some(name) => self.container_name_with_instance(name),
            None => self.container_name(),
        };
        let container_exists = DockerOps::container_exists(Some(self.executable), &container_name)
            .map_err(|e| warn!("Failed to check existing containers (continuing): {}", e))
            .unwrap_or(false);

        if container_exists {
            return match instance_name {
                Some(name) => self.handle_existing_container_with_instance(name),
                None => self.handle_existing_container(),
            };
        }

        // Only setup audio if it's enabled in the configuration
        if let Some(audio_service) = self.config.services.get("audio") {
            if audio_service.enabled {
                #[cfg(target_os = "macos")]
                if let Err(e) = MacOSAudioManager::setup() {
                    warn!("Audio setup failed: {}", e);
                }
                #[cfg(not(target_os = "macos"))]
                MacOSAudioManager::setup();
            }
        }

        let _vm_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("vm-project");

        // Messages are now handled at the command level for consistency

        // Step 1: Filter pipx-managed packages from pip_packages
        let modified_config = self.prepare_config_for_build()?;

        // Step 2: Prepare build context with embedded resources
        let build_ops = BuildOperations::new(&modified_config, self.temp_dir);
        let (build_context, base_image) = build_ops.prepare_build_context()?;

        // Step 2.5: Ensure Docker networks exist (create them if needed)
        if let Some(networking) = &modified_config.networking {
            if !networking.networks.is_empty() {
                info!("Ensuring Docker networks exist: {:?}", networking.networks);
                DockerOps::ensure_networks_exist(Some(self.executable), &networking.networks)?;
            }
        }

        // Step 3: Generate docker-compose.yml with build context and modified config
        let compose_ops = ComposeOperations::new(
            &modified_config,
            self.temp_dir,
            self.project_dir,
            self.executable,
        );
        let compose_path = match instance_name {
            Some(name) => {
                compose_ops.write_docker_compose_with_instance(&build_context, name, context)?
            }
            None => compose_ops.write_docker_compose(&build_context, context)?,
        };

        // Pre-flight validation: Check for orphaned service containers
        // Now that docker-compose.yml exists, we can check for conflicts
        // Pass instance_name to avoid flagging the target instance container as orphaned
        self.check_for_orphaned_containers(instance_name, context)?;

        // Step 4: Gather build arguments for packages
        let build_args = build_ops.gather_build_args(&base_image);

        // Step 5: Build with all package arguments
        let base_compose_args = ComposeCommand::build_args(&compose_path, "build", &[])?;

        // Combine compose args with dynamic build args
        let mut all_args = Vec::with_capacity(base_compose_args.len() + build_args.len());
        all_args.extend(base_compose_args.iter().map(|s| s.as_str()));
        all_args.extend(build_args.iter().map(|s| s.as_str()));

        // Debug logging for Docker build troubleshooting
        vm_dbg!("Docker build command: docker {}", all_args.join(" "));
        vm_dbg!("Build context directory: {}", build_context.display());
        if let Ok(entries) = fs::read_dir(&build_context) {
            let file_count = entries.count();
            vm_dbg!("Build context contains {} files/directories", file_count);
        }

        stream_command_visible(self.executable, &all_args).map_err(|e| {
            match instance_name {
                Some(name) => VmError::Internal(format!(
                    "Docker build failed for project '{}' instance '{}'. Check that Docker is running and build context is valid: {}",
                    self.project_name(),
                    name,
                    e
                )),
                None => VmError::Internal(format!(
                    "Docker build failed for project '{}'. Check that Docker is running and build context is valid: {}",
                    self.project_name(),
                    e
                )),
            }
        })?;

        // Step 6: Start containers
        let args = ComposeCommand::build_args(&compose_path, "up", &["-d"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        stream_command_visible(self.executable, &args_refs).map_err(|e| {
            let error_msg = e.to_string();

            // Detect container name conflicts (orphaned containers from failed creation)
            // Heuristic: Docker/Compose have used these error strings consistently for 5+ years.
            // We show the full error regardless, so users aren't blind if the heuristic fails.
            if error_msg.contains("is already in use") || error_msg.contains("Conflict") {
                eprintln!("\n⚠️  Container name conflict detected");
                eprintln!("\n   Docker error:");
                eprintln!("   {}", error_msg);  // Show actual error - contains conflicting container name

                eprintln!("\n   This usually means a previous creation failed partway through.");

                // Show what's actually running for this project
                self.list_project_containers_for_user();

                eprintln!("\n💡 Recommended fix:");
                eprintln!("      vm destroy --force");
                if let Some(name) = instance_name {
                    eprintln!("      vm create --instance {}", name);
                } else {
                    eprintln!("      vm create");
                }
                eprintln!();

                VmError::Internal(format!(
                    "Container name conflict detected. See error details above.\n\nOriginal error: {}",
                    error_msg
                ))
            } else {
                VmError::Internal(format!(
                    "Failed to start container '{}': {}",
                    container_name,
                    e
                ))
            }
        })?;

        // Provision the container
        match instance_name {
            Some(name) => self.provision_container_with_instance_and_context(name, context)?,
            None => self.provision_container_with_context(context)?,
        }

        Ok(())
    }

    /// Create a container with a specific instance name
    #[must_use = "container creation results should be handled"]
    pub fn create_container_with_instance(&self, instance_name: &str) -> Result<()> {
        self.create_container_with_instance_and_context(instance_name, &ProviderContext::default())
    }

    /// Create a container with a specific instance name and context
    #[must_use = "container creation results should be handled"]
    pub fn create_container_with_instance_and_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        self.create_container_impl(Some(instance_name), context)
    }

    #[must_use = "existing container handling results should be checked"]
    fn handle_existing_container(&self) -> Result<()> {
        let container_name = self.container_name();

        // Check if it's running
        let is_running = DockerOps::is_container_running(Some(self.executable), &container_name)
            .map_err(|e| warn!("Failed to check running containers (continuing): {}", e))
            .unwrap_or(false);

        let status = if is_running { "running" } else { "stopped" };
        warn!(
            "Container '{}' already exists (status: {}).",
            container_name, status
        );

        // Check if we're in an interactive terminal
        if std::io::stdin().is_terminal() {
            let option1 = if is_running {
                MESSAGES.service.docker_container_exists_running
            } else {
                MESSAGES.service.docker_container_exists_stopped
            };

            info!(
                "{}",
                msg!(
                    MESSAGES.service.docker_container_exists_prompt,
                    option1 = option1
                )
            );
            print!("{}", MESSAGES.service.docker_container_choice_prompt);
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim() {
                "1" => {
                    if is_running {
                        info!("Using existing running container.");
                        Ok(())
                    } else {
                        info!("{}", MESSAGES.service.docker_container_starting);
                        self.start_container(None)
                    }
                }
                "2" => {
                    info!("{}", MESSAGES.service.docker_container_recreating);
                    self.destroy_container(None)?;
                    // Continue with creation below
                    self.create_container()
                }
                _ => {
                    error!("Operation cancelled.");
                    Ok(())
                }
            }
        } else {
            // Non-interactive mode: fail with informative error
            Err(VmError::Internal(format!(
                "Container '{container_name}' already exists. In non-interactive mode, please use:\n\
                 - 'vm start' to start the existing container\n\
                 - 'vm destroy' followed by 'vm create' to recreate it"
            )))
        }
    }

    /// Handle existing container with custom instance name
    #[must_use = "existing container handling results should be checked"]
    fn handle_existing_container_with_instance(&self, instance_name: &str) -> Result<()> {
        let container_name = self.container_name_with_instance(instance_name);

        // Check if it's running
        let is_running = DockerOps::is_container_running(Some(self.executable), &container_name)
            .map_err(|e| warn!("Failed to check running containers (continuing): {}", e))
            .unwrap_or(false);

        let status = if is_running { "running" } else { "stopped" };
        warn!(
            "Container '{}' already exists (status: {}).",
            container_name, status
        );

        // Check if we're in an interactive terminal
        if std::io::stdin().is_terminal() {
            let option1 = if is_running {
                MESSAGES.service.docker_container_exists_running
            } else {
                MESSAGES.service.docker_container_exists_stopped
            };

            info!(
                "{}",
                msg!(
                    MESSAGES.service.docker_container_exists_prompt,
                    option1 = option1
                )
            );
            print!("{}", MESSAGES.service.docker_container_choice_prompt);
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim() {
                "1" => {
                    if is_running {
                        info!("Using existing running container.");
                        Ok(())
                    } else {
                        info!("{}", MESSAGES.service.docker_container_starting);
                        self.start_container(Some(&container_name))
                    }
                }
                "2" => {
                    info!("{}", MESSAGES.service.docker_container_recreating);
                    self.destroy_container(Some(&container_name))?;
                    // Continue with creation below
                    self.create_container_with_instance(instance_name)
                }
                _ => {
                    error!("Operation cancelled.");
                    Ok(())
                }
            }
        } else {
            // Non-interactive mode: fail with informative error
            Err(VmError::Internal(format!(
                "Container '{container_name}' already exists. In non-interactive mode, please use:\n\
                 - 'vm start {container_name}' to start the existing container\n\
                 - 'vm destroy {container_name}' followed by 'vm create --instance {instance_name}' to recreate it"
            )))
        }
    }

    #[must_use = "config preparation results should be checked"]
    fn prepare_config_for_build(&self) -> Result<Cow<'_, VmConfig>> {
        let pipx_managed_packages = if let Some(pipx_json) = self.get_pipx_json()? {
            self.extract_pipx_managed_packages(&pipx_json)
        } else {
            std::collections::HashSet::new()
        };

        // Only clone if we need to modify pip_packages
        if pipx_managed_packages.is_empty() {
            Ok(Cow::Borrowed(self.config))
        } else {
            let mut config = self.config.clone();
            config.pip_packages = config
                .pip_packages
                .iter()
                .filter(|pkg| !pipx_managed_packages.contains(*pkg))
                .cloned()
                .collect();
            Ok(Cow::Owned(config))
        }
    }

    #[must_use = "config copy preparation results should be checked"]
    pub(super) fn prepare_config_for_copy(
        &self,
        container_pipx_packages: &[String],
    ) -> Result<Cow<'_, VmConfig>> {
        let needs_pip_clear = !self.config.pip_packages.is_empty();
        let needs_extra_config = !container_pipx_packages.is_empty();

        if !needs_pip_clear && !needs_extra_config {
            return Ok(Cow::Borrowed(self.config));
        }

        let mut config = self.config.clone();

        // Always clear pip_packages if we processed any pipx packages, regardless of whether they were included
        if needs_pip_clear {
            config.pip_packages = vec![];
        }

        // Add PyPI packages list to config for Ansible
        if needs_extra_config {
            config.extra_config.insert(
                "container_pipx_packages".to_string(),
                serde_json::to_value(container_pipx_packages).map_err(|e| {
                    VmError::Internal(format!(
                        "Failed to serialize pipx package list for container configuration: {e}"
                    ))
                })?,
            );
        }

        Ok(Cow::Owned(config))
    }

    #[must_use = "temp config preparation results should be checked"]
    pub(super) fn prepare_temp_config(&self) -> Result<Cow<'_, VmConfig>> {
        if let Some(ref project) = self.config.project {
            if project.name.as_deref() == Some("vm-temp") {
                // Already has the right name, no need to clone
                return Ok(Cow::Borrowed(self.config));
            }
        }

        // Need to clone to modify project name
        let mut config = self.config.clone();
        if let Some(ref mut project) = config.project {
            project.name = Some("vm-temp".to_owned());
        } else {
            // Create project config if it doesn't exist
            config.project = Some(vm_config::config::ProjectConfig {
                name: Some("vm-temp".to_owned()),
                ..Default::default()
            });
        }
        Ok(Cow::Owned(config))
    }

    #[must_use = "config preparation results should be checked"]
    pub(super) fn prepare_and_copy_config(&self) -> Result<()> {
        let container_pipx_packages = if let Some(pipx_json) = self.get_pipx_json()? {
            self.categorize_pipx_packages(&pipx_json)
        } else {
            Vec::new()
        };

        let config_for_copy = self.prepare_config_for_copy(&container_pipx_packages)?;

        let config_json = config_for_copy.to_json()?;
        let temp_config_path = self.temp_dir.join("vm-config.json");
        fs::write(&temp_config_path, config_json).map_err(|e| {
            VmError::Internal(format!(
                "Failed to write configuration to {}: {}",
                temp_config_path.display(),
                e
            ))
        })?;
        let source = BuildOperations::path_to_string(&temp_config_path)?;
        let destination = format!("{}:{}", self.container_name(), super::TEMP_CONFIG_PATH);
        let copy_result = DockerOps::copy(Some(self.executable), source, &destination);
        if copy_result.is_err() {
            return Err(VmError::Internal(format!(
                "Failed to copy VM configuration to container '{}'. Container may not be running or accessible",
                self.container_name()
            )));
        }
        Ok(())
    }

    /// Helper to list all containers for this project (used in error messages).
    fn list_project_containers_for_user(&self) {
        if let Ok(containers) =
            DockerOps::list_containers(Some(self.executable), true, "{{.Names}}")
        {
            let project_prefix = format!("{}-", self.project_name());
            let project_containers: Vec<&str> = containers
                .lines()
                .map(str::trim)
                .filter(|name| name.starts_with(&project_prefix))
                .collect();

            if !project_containers.is_empty() {
                eprintln!("\n   Existing containers for {}:", self.project_name());
                for container in project_containers {
                    eprintln!("   • {}", container);
                }
            }
        }
    }

    /// Check for orphaned service containers from previous failed creation attempts.
    ///
    /// Service containers (postgresql, redis, etc.) may be left running if the main
    /// container creation fails partway through. This pre-flight check warns users
    /// early and provides actionable guidance.
    ///
    /// IMPORTANT: This only flags known SERVICE containers, not instance containers.
    /// Multiple instances (myproject-foo, myproject-bar) are intentional and should coexist.
    ///
    /// Uses Docker to find service containers matching known patterns from template.yml:
    /// - PostgreSQL: {project}-postgres (explicit container_name)
    /// - Redis: {project}-redis-1 (Compose default, if added to template)
    /// - MongoDB: {project}-mongodb-1 (Compose default, if added to template)
    fn check_for_orphaned_containers(
        &self,
        _instance_name: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        // Query Docker for all containers to find orphaned services
        let all_containers = DockerOps::list_containers(Some(self.executable), true, "{{.Names}}")?;

        // Use ComposeOperations to get expected service names based on current config
        // Note: This uses the current config, so if a service was removed from config
        // but still exists as a container, it might not be caught here.
        // However, checking all possible legacy names is brittle.
        // For now, we check the services enabled in the current config.
        let compose_ops = ComposeOperations::new(
            self.config,
            self.temp_dir,
            self.project_dir,
            self.executable,
        );
        let service_patterns = compose_ops.get_expected_service_containers();

        let mut orphaned = Vec::new();

        for container_name in all_containers.lines().map(str::trim) {
            // Only flag known service containers, not instance containers
            if service_patterns
                .iter()
                .any(|pattern| container_name == pattern)
            {
                orphaned.push(container_name.to_string());
            }
        }

        // If orphaned containers found, handle based on preserve_services flag
        if !orphaned.is_empty() {
            if context.preserve_services {
                // Default behavior: warn and continue to preserve data
                warn!(
                    "Found existing service containers; continuing to reuse them: {}",
                    orphaned.join(", ")
                );
                eprintln!("\n⚠️  Existing service containers detected (will reuse data):\n");
                for container in &orphaned {
                    eprintln!("   • {}", container);
                }
                eprintln!("\n💡 We'll reuse these to preserve your data.");
                eprintln!("   If you want a fresh database, remove them first:");
                for container in &orphaned {
                    eprintln!("      docker rm -f {}", container);
                }
                eprintln!("   Or run: vm destroy --force\n");
            } else {
                // preserve_services=false: error on existing service containers
                let container_list = orphaned.join(", ");
                return Err(VmError::Provider(format!(
                    "Found existing service containers that would conflict: {}. \
                    Use 'vm destroy --force' to remove them, or use '--preserve-services' to reuse them.",
                    container_list
                )));
            }
        }

        Ok(())
    }
}
