//! Container creation and setup
use is_terminal::IsTerminal;
use std::borrow::Cow;
use std::fs;
use std::io::{self, Write};

use super::LifecycleOperations;
use crate::{
    audio::MacOSAudioManager,
    context::ProviderContext,
    docker::{build::BuildOperations, compose::ComposeOperations, ComposeCommand, DockerOps},
};
use vm_cli::msg;
use vm_config::config::VmConfig;
use vm_core::{
    command_stream::stream_command,
    error::{Result, VmError},
    vm_dbg, vm_error, vm_println, vm_success, vm_warning,
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

        // Create worktrees directory if enabled (must exist before Docker mounts it)
        if self.config.worktrees.as_ref().is_some_and(|w| w.enabled)
            || context
                .global_config
                .as_ref()
                .is_some_and(|g| g.worktrees.enabled)
        {
            let compose_ops = ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
            if let Some(worktrees_path) = compose_ops.get_worktrees_host_path(context) {
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

        // Check platform support for worktrees (Windows native not supported)
        #[cfg(target_os = "windows")]
        {
            if self.config.worktrees.as_ref().is_some_and(|w| w.enabled)
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

                vm_success!("✓ WSL2 detected - worktrees will work correctly");
            }
        }

        // Check if container already exists
        let container_name = self.container_name();
        let container_exists = DockerOps::container_exists(&container_name)
            .map_err(|e| vm_warning!("Failed to check existing containers (continuing): {}", e))
            .unwrap_or(false);

        if container_exists {
            return self.handle_existing_container();
        }

        // Only setup audio if it's enabled in the configuration
        if let Some(audio_service) = self.config.services.get("audio") {
            if audio_service.enabled {
                #[cfg(target_os = "macos")]
                if let Err(e) = MacOSAudioManager::setup() {
                    vm_warning!("Audio setup failed: {}", e);
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
        let build_context = build_ops.prepare_build_context()?;

        // Step 3: Generate docker-compose.yml with build context and modified config
        let compose_ops = ComposeOperations::new(&modified_config, self.temp_dir, self.project_dir);
        let compose_path = compose_ops.write_docker_compose(&build_context, context)?;

        // Step 3: Gather build arguments for packages
        let build_args = build_ops.gather_build_args();

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

        stream_command("docker", &all_args).map_err(|e| {
            VmError::Internal(format!(
                "Docker build failed for project '{}'. Check that Docker is running and build context is valid: {}",
                self.project_name(),
                e
            ))
        })?;

        // Step 5: Start containers
        let args = ComposeCommand::build_args(&compose_path, "up", &["-d"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command("docker", &args_refs).map_err(|e| {
            VmError::Internal(format!(
                "Failed to start container '{}'. Container may have failed to start properly: {}",
                self.container_name(),
                e
            ))
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

        // Create worktrees directory if enabled (must exist before Docker mounts it)
        if self.config.worktrees.as_ref().is_some_and(|w| w.enabled)
            || context
                .global_config
                .as_ref()
                .is_some_and(|g| g.worktrees.enabled)
        {
            let compose_ops = ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
            if let Some(worktrees_path) = compose_ops.get_worktrees_host_path(context) {
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

        // Check platform support for worktrees (Windows native not supported)
        #[cfg(target_os = "windows")]
        {
            if self.config.worktrees.as_ref().is_some_and(|w| w.enabled)
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

                vm_success!("✓ WSL2 detected - worktrees will work correctly");
            }
        }

        // Check if container already exists (with custom instance name)
        let container_name = self.container_name_with_instance(instance_name);
        let container_exists = DockerOps::container_exists(&container_name)
            .map_err(|e| vm_warning!("Failed to check existing containers (continuing): {}", e))
            .unwrap_or(false);

        if container_exists {
            return self.handle_existing_container_with_instance(instance_name);
        }

        // Only setup audio if it's enabled in the configuration
        if let Some(audio_service) = self.config.services.get("audio") {
            if audio_service.enabled {
                #[cfg(target_os = "macos")]
                if let Err(e) = MacOSAudioManager::setup() {
                    vm_warning!("Audio setup failed: {}", e);
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
        let build_context = build_ops.prepare_build_context()?;

        // Step 3: Generate docker-compose.yml with custom instance name
        let compose_ops = ComposeOperations::new(&modified_config, self.temp_dir, self.project_dir);
        let compose_path = compose_ops.write_docker_compose_with_instance(
            &build_context,
            instance_name,
            context,
        )?;

        // Step 3: Gather build arguments for packages
        let build_args = build_ops.gather_build_args();

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

        stream_command("docker", &all_args).map_err(|e| {
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
        stream_command("docker", &args_refs).map_err(|e| {
            VmError::Internal(format!(
                "Failed to start container '{}'. Container may have failed to start properly: {}",
                container_name, e
            ))
        })?;

        self.provision_container_with_instance_and_context(instance_name, context)?;

        Ok(())
    }

    #[must_use = "existing container handling results should be checked"]
    fn handle_existing_container(&self) -> Result<()> {
        let container_name = self.container_name();

        // Check if it's running
        let is_running = DockerOps::is_container_running(&container_name)
            .map_err(|e| vm_warning!("Failed to check running containers (continuing): {}", e))
            .unwrap_or(false);

        let status = if is_running { "running" } else { "stopped" };
        vm_warning!(
            "Container '{}' already exists (status: {}).",
            container_name,
            status
        );

        // Check if we're in an interactive terminal
        if io::stdin().is_terminal() {
            let option1 = if is_running {
                MESSAGES.docker_container_exists_running
            } else {
                MESSAGES.docker_container_exists_stopped
            };

            vm_println!(
                "{}",
                msg!(MESSAGES.docker_container_exists_prompt, option1 = option1)
            );
            print!("{}", MESSAGES.docker_container_choice_prompt);
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim() {
                "1" => {
                    if is_running {
                        vm_success!("Using existing running container.");
                        Ok(())
                    } else {
                        vm_println!("{}", MESSAGES.docker_container_starting);
                        self.start_container(None)
                    }
                }
                "2" => {
                    vm_println!("{}", MESSAGES.docker_container_recreating);
                    self.destroy_container(None)?;
                    // Continue with creation below
                    self.create_container()
                }
                _ => {
                    vm_error!("Operation cancelled.");
                    Ok(())
                }
            }
        } else {
            // Non-interactive mode: fail with informative error
            Err(VmError::Internal(format!(
                "Container '{}' already exists. In non-interactive mode, please use:\n\
                 - 'vm start' to start the existing container\n\
                 - 'vm destroy' followed by 'vm create' to recreate it",
                container_name
            )))
        }
    }

    /// Handle existing container with custom instance name
    #[must_use = "existing container handling results should be checked"]
    fn handle_existing_container_with_instance(&self, instance_name: &str) -> Result<()> {
        let container_name = self.container_name_with_instance(instance_name);

        // Check if it's running
        let is_running = DockerOps::is_container_running(&container_name)
            .map_err(|e| vm_warning!("Failed to check running containers (continuing): {}", e))
            .unwrap_or(false);

        let status = if is_running { "running" } else { "stopped" };
        vm_warning!(
            "Container '{}' already exists (status: {}).",
            container_name,
            status
        );

        // Check if we're in an interactive terminal
        if io::stdin().is_terminal() {
            let option1 = if is_running {
                MESSAGES.docker_container_exists_running
            } else {
                MESSAGES.docker_container_exists_stopped
            };

            vm_println!(
                "{}",
                msg!(MESSAGES.docker_container_exists_prompt, option1 = option1)
            );
            print!("{}", MESSAGES.docker_container_choice_prompt);
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim() {
                "1" => {
                    if is_running {
                        vm_success!("Using existing running container.");
                        Ok(())
                    } else {
                        vm_println!("{}", MESSAGES.docker_container_starting);
                        self.start_container(Some(&container_name))
                    }
                }
                "2" => {
                    vm_println!("{}", MESSAGES.docker_container_recreating);
                    self.destroy_container(Some(&container_name))?;
                    // Continue with creation below
                    self.create_container_with_instance(instance_name)
                }
                _ => {
                    vm_error!("Operation cancelled.");
                    Ok(())
                }
            }
        } else {
            // Non-interactive mode: fail with informative error
            Err(VmError::Internal(format!(
                "Container '{}' already exists. In non-interactive mode, please use:\n\
                 - 'vm start {}' to start the existing container\n\
                 - 'vm destroy {}' followed by 'vm create --instance {}' to recreate it",
                container_name, container_name, container_name, instance_name
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
                        "Failed to serialize pipx package list for container configuration: {}",
                        e
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
}
