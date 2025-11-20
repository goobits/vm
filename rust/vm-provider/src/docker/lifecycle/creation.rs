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
        self.check_daemon_is_running()?;
        self.handle_potential_issues();
        self.check_docker_build_requirements();

        // Pre-flight validation: Check for orphaned service containers
        self.check_for_orphaned_containers()?;

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
        let container_name = self.container_name();
        let container_exists = DockerOps::container_exists(&container_name)
            .map_err(|e| warn!("Failed to check existing containers (continuing): {}", e))
            .unwrap_or(false);

        if container_exists {
            return self.handle_existing_container();
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
                DockerOps::ensure_networks_exist(&networking.networks)?;
            }
        }

        // Step 3: Generate docker-compose.yml with build context and modified config
        let compose_ops = ComposeOperations::new(&modified_config, self.temp_dir, self.project_dir);
        let compose_path = compose_ops.write_docker_compose(&build_context, context)?;

        // Step 3: Gather build arguments for packages
        let build_args = build_ops.gather_build_args(&base_image);

        // Step 4: Build with all package arguments
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

        stream_command_visible("docker", &all_args).map_err(|e| {
            VmError::Internal(format!(
                "Docker build failed for project '{}'. Check that Docker is running and build context is valid: {}",
                self.project_name(),
                e
            ))
        })?;

        // Step 5: Start containers
        let args = ComposeCommand::build_args(&compose_path, "up", &["-d"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let container_name = self.container_name();

        stream_command_visible("docker", &args_refs).map_err(|e| {
            let error_msg = e.to_string();

            // Detect container name conflicts (orphaned containers from failed creation)
            // Note: Error messages from Docker Compose are relatively stable, but this is a heuristic.
            // The pre-flight check (check_for_orphaned_containers) should catch most cases earlier.
            // This is a fallback for edge cases or if the pre-flight check misses something.
            if error_msg.contains("is already in use") || error_msg.contains("Conflict") {
                eprintln!("\n⚠️  Container name conflict detected");
                eprintln!("   This usually means a previous creation failed partway through,");
                eprintln!("   leaving orphaned containers from this project.");
                eprintln!("\n💡 Recommended fix:");
                eprintln!("      vm destroy --force");
                eprintln!("      vm create");
                eprintln!("\n   Or inspect what's running:");
                eprintln!("      docker ps -a | grep {}", self.project_name());
                eprintln!("\n   Then manually remove conflicting containers:");
                eprintln!("      docker rm -f <container-name>");
                eprintln!();

                VmError::Internal(format!(
                    "Container name conflict. Previous creation may have failed. Clean up first.\n\nOriginal error: {}",
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

        self.provision_container_with_context(context)?;

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
        self.check_daemon_is_running()?;
        self.handle_potential_issues();
        self.check_docker_build_requirements();

        // Pre-flight validation: Check for orphaned service containers
        self.check_for_orphaned_containers()?;

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

        // Check if container already exists (with custom instance name)
        let container_name = self.container_name_with_instance(instance_name);
        let container_exists = DockerOps::container_exists(&container_name)
            .map_err(|e| warn!("Failed to check existing containers (continuing): {}", e))
            .unwrap_or(false);

        if container_exists {
            return self.handle_existing_container_with_instance(instance_name);
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
                DockerOps::ensure_networks_exist(&networking.networks)?;
            }
        }

        // Step 3: Generate docker-compose.yml with custom instance name
        let compose_ops = ComposeOperations::new(&modified_config, self.temp_dir, self.project_dir);
        let compose_path = compose_ops.write_docker_compose_with_instance(
            &build_context,
            instance_name,
            context,
        )?;

        // Step 3: Gather build arguments for packages
        let build_args = build_ops.gather_build_args(&base_image);

        // Step 4: Build with all package arguments
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

        stream_command_visible("docker", &all_args).map_err(|e| {
            VmError::Internal(format!(
                "Docker build failed for project '{}' instance '{}'. Check that Docker is running and build context is valid: {}",
                self.project_name(),
                instance_name,
                e
            ))
        })?;

        // Step 5: Start containers
        let args = ComposeCommand::build_args(&compose_path, "up", &["-d"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        stream_command_visible("docker", &args_refs).map_err(|e| {
            let error_msg = e.to_string();

            // Detect container name conflicts (orphaned containers from failed creation)
            // Note: Error messages from Docker Compose are relatively stable, but this is a heuristic.
            // The pre-flight check (check_for_orphaned_containers) should catch most cases earlier.
            // This is a fallback for edge cases or if the pre-flight check misses something.
            if error_msg.contains("is already in use") || error_msg.contains("Conflict") {
                eprintln!("\n⚠️  Container name conflict detected");
                eprintln!("   This usually means a previous creation failed partway through,");
                eprintln!("   leaving orphaned containers from this project.");
                eprintln!("\n💡 Recommended fix:");
                eprintln!("      vm destroy --force");
                eprintln!("      vm create --instance {}", instance_name);
                eprintln!("\n   Or inspect what's running:");
                eprintln!("      docker ps -a | grep {}", self.project_name());
                eprintln!("\n   Then manually remove conflicting containers:");
                eprintln!("      docker rm -f <container-name>");
                eprintln!();

                VmError::Internal(format!(
                    "Container name conflict. Previous creation may have failed. Clean up first.\n\nOriginal error: {}",
                    error_msg
                ))
            } else {
                VmError::Internal(format!(
                    "Failed to start container '{container_name}': {e}"
                ))
            }
        })?;

        self.provision_container_with_instance_and_context(instance_name, context)?;

        Ok(())
    }

    #[must_use = "existing container handling results should be checked"]
    fn handle_existing_container(&self) -> Result<()> {
        let container_name = self.container_name();

        // Check if it's running
        let is_running = DockerOps::is_container_running(&container_name)
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
        let is_running = DockerOps::is_container_running(&container_name)
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
        let copy_result = DockerOps::copy(source, &destination);
        if copy_result.is_err() {
            return Err(VmError::Internal(format!(
                "Failed to copy VM configuration to container '{}'. Container may not be running or accessible",
                self.container_name()
            )));
        }
        Ok(())
    }

    /// Check for orphaned service containers from previous failed creation attempts.
    ///
    /// Service containers (postgresql, redis, etc.) may be left running if the main
    /// container creation fails partway through. This pre-flight check warns users
    /// early and provides actionable guidance.
    fn check_for_orphaned_containers(&self) -> Result<()> {
        let project_name = self.project_name();
        let mut orphaned = Vec::new();

        // Iterate over enabled services and check for orphaned containers
        // Container naming patterns must match template.yml definitions
        for (service_name, service_config) in &self.config.services {
            if !service_config.enabled {
                continue;
            }

            // Map service names to their container naming patterns from template.yml
            // Only PostgreSQL currently has an explicit container_name (template.yml:214)
            // Future services should be added here as they're defined in the template
            let container_name = match service_name.as_str() {
                "postgresql" => Some(format!("{}-postgres", project_name)),
                // Redis/MongoDB would use Compose defaults ({project}-{service}-1) unless
                // template.yml is updated to give them explicit container_name like PostgreSQL
                _ => None,
            };

            if let Some(name) = container_name {
                if DockerOps::container_exists(&name).unwrap_or(false) {
                    orphaned.push(name);
                }
            }
        }

        // If orphaned containers found, warn user with actionable guidance
        if !orphaned.is_empty() {
            warn!("Found orphaned service containers from previous failed creation");
            eprintln!("\n⚠️  Warning: Orphaned service containers detected");
            eprintln!("   Previous VM creation may have failed partway through,");
            eprintln!("   leaving these service containers running:\n");
            for container in &orphaned {
                eprintln!("   • {}", container);
            }
            eprintln!("\n💡 These containers may cause name conflicts during creation.");
            eprintln!("   To clean up and proceed:");
            eprintln!("      vm destroy --force");
            eprintln!("      vm create\n");
            eprintln!("   Or manually remove them:");
            for container in &orphaned {
                eprintln!("      docker rm -f {}", container);
            }
            eprintln!();

            return Err(VmError::Internal(format!(
                "Orphaned service containers found: {}. Clean them up first to avoid conflicts.",
                orphaned.join(", ")
            )));
        }

        Ok(())
    }
}
