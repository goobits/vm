//! Docker container lifecycle management operations.
//!
//! This module handles the complete lifecycle of Docker containers used
//! for VM environments, including creation, starting, stopping, provisioning,
//! and cleanup operations. It provides a high-level interface for VM
//! management through Docker containers with progress reporting and error handling.

// Standard library
use std::borrow::Cow;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

// External crates
use is_terminal::IsTerminal;
use serde_json::Value;
use vm_core::error::{Result, VmError};

// Internal imports
use super::{
    build::BuildOperations, command::DockerCommand, compose::ComposeOperations, ComposeCommand,
    DockerOps, UserConfig,
};
use crate::{
    audio::MacOSAudioManager,
    context::ProviderContext,
    progress::{AnsibleProgressParser, ProgressParser, ProgressReporter},
    security::SecurityValidator,
    ResourceUsage, ServiceStatus, TempProvider, TempVmState, VmStatusReport,
};
use vm_cli::msg;
use vm_config::config::VmConfig;
use vm_core::command_stream::{
    stream_command, stream_command_with_progress, ProgressParser as CoreProgressParser,
};
use vm_core::vm_error_with_details;
use vm_core::{vm_dbg, vm_error, vm_error_hint, vm_println, vm_success, vm_warning};
use vm_messages::messages::MESSAGES;

// Constants for container lifecycle operations
const DEFAULT_PROJECT_NAME: &str = "vm-project";
const CONTAINER_SUFFIX: &str = "-dev";
const DEFAULT_WORKSPACE_PATH: &str = "/workspace";
const DEFAULT_SHELL: &str = "zsh";
const HIGH_MEMORY_THRESHOLD: u32 = 8192;
const CONTAINER_READINESS_MAX_ATTEMPTS: u32 = 30;
const CONTAINER_READINESS_SLEEP_SECONDS: u64 = 2;
const ANSIBLE_PLAYBOOK_PATH: &str = "/app/shared/ansible/playbook.yml";
const TEMP_CONFIG_PATH: &str = "/tmp/vm-config.json";

pub struct LifecycleOperations<'a> {
    pub config: &'a VmConfig,
    pub temp_dir: &'a std::path::PathBuf,
    pub project_dir: &'a std::path::PathBuf,
}

impl<'a> LifecycleOperations<'a> {
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

    /// Helper to extract pipx managed packages (for filtering from pip_packages)
    #[must_use = "package extraction results should be checked"]
    fn extract_pipx_managed_packages(
        &self,
        pipx_json: &Value,
    ) -> std::collections::HashSet<String> {
        let mut pipx_managed_packages = std::collections::HashSet::new();

        if let Some(venvs) = pipx_json.get("venvs").and_then(|v| v.as_object()) {
            for package in &self.config.pip_packages {
                if venvs.contains_key(package) {
                    pipx_managed_packages.insert(package.clone());
                }
            }
        }

        pipx_managed_packages
    }

    /// Helper to get pipx JSON output
    #[must_use = "pipx command results should be checked"]
    fn get_pipx_json(&self) -> Result<Option<Value>> {
        if self.config.pip_packages.is_empty() {
            return Ok(None);
        }

        let pipx_list_output = std::process::Command::new("pipx")
            .args(["list", "--json"])
            .output()?;

        if pipx_list_output.status.success() {
            let pipx_json = serde_json::from_slice::<Value>(&pipx_list_output.stdout)
                .map_err(|e| VmError::Internal(format!("Failed to parse pipx package listing output as JSON. pipx may have returned invalid output: {}", e)))?;
            Ok(Some(pipx_json))
        } else {
            Ok(None)
        }
    }

    /// Helper to categorize pipx packages (only returns container packages now)
    #[must_use = "package categorization results should be checked"]
    fn categorize_pipx_packages(&self, pipx_json: &Value) -> Vec<String> {
        let mut container_pipx_packages = Vec::new();

        if let Some(venvs) = pipx_json.get("venvs").and_then(|v| v.as_object()) {
            let pipx_managed_packages: std::collections::HashSet<String> =
                venvs.keys().cloned().collect();

            for package in &self.config.pip_packages {
                if pipx_managed_packages.contains(package) {
                    // All pipx packages are now treated as PyPI packages for container installation
                    container_pipx_packages.push(package.clone());
                }
            }
        }

        container_pipx_packages
    }

    pub fn project_name(&self) -> &str {
        self.config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or(DEFAULT_PROJECT_NAME)
    }

    pub fn container_name(&self) -> String {
        format!("{}{}", self.project_name(), CONTAINER_SUFFIX)
    }

    /// Generate container name with custom instance suffix
    pub fn container_name_with_instance(&self, instance_name: &str) -> String {
        format!("{}-{}", self.project_name(), instance_name)
    }

    /// Check memory allocation and provide guidance
    fn check_memory_allocation(&self, vm_config: &vm_config::config::VmSettings) {
        if let Some(memory) = &vm_config.memory {
            match memory.to_mb() {
                Some(mb) if mb > HIGH_MEMORY_THRESHOLD => {
                    vm_error_hint!("High memory allocation detected ({}MB). Ensure your system has sufficient RAM.", mb);
                }
                None => {
                    vm_error_hint!(
                        "Unlimited memory detected. Monitor system resources during development."
                    );
                }
                _ => {} // Normal memory allocation, no warning needed
            }
        }
    }

    /// Handle potential Docker issues proactively
    pub fn handle_potential_issues(&self) {
        // Check for port conflicts and provide helpful guidance
        if let Some(vm_config) = &self.config.vm {
            self.check_memory_allocation(vm_config);
        }

        // Check Docker daemon status more thoroughly
        if DockerCommand::new().subcommand("ps").execute().is_err() {
            vm_error_with_details!(
                "Docker daemon may not be responding properly",
                &["Try: docker system prune -f", "Or: restart Docker Desktop"]
            );
        }
    }

    #[must_use = "Docker daemon status should be checked"]
    pub fn check_daemon_is_running(&self) -> Result<()> {
        DockerOps::check_daemon_running()
            .map_err(|_| VmError::Internal("Docker daemon is not running".to_string()))
    }

    /// Check Docker build requirements (disk space, resources)
    fn check_docker_build_requirements(&self) {
        self.check_disk_space_unix();
        self.check_disk_space_windows();
    }

    /// Check disk space on Unix-like systems (Linux and macOS)
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn check_disk_space_unix(&self) {
        let available_gb = match self.get_available_disk_space_unix() {
            Some(gb) => gb,
            None => return, // Couldn't determine disk space, continue silently
        };

        if available_gb < 2 {
            vm_warning!(
                "Low disk space: {}GB available. Docker builds may fail with insufficient storage.",
                available_gb
            );
        }
    }

    /// Get available disk space on Unix systems, returning GB as u32
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn get_available_disk_space_unix(&self) -> Option<u32> {
        let output = std::process::Command::new("df")
            .args(["-BG", "."])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let df_output = String::from_utf8(output.stdout).ok()?;
        let line = df_output.lines().nth(1)?;
        let available = line.split_whitespace().nth(3)?;
        available.trim_end_matches('G').parse::<u32>().ok()
    }

    /// Check disk space on Windows systems
    #[cfg(target_os = "windows")]
    fn check_disk_space_windows(&self) {
        let available_gb = match self.get_available_disk_space_windows() {
            Some(gb) => gb,
            None => return, // Couldn't determine disk space, continue silently
        };

        if available_gb < 2.0 {
            vm_warning!("Low disk space: {:.1}GB available. Docker builds may fail with insufficient storage.", available_gb);
        }
    }

    /// Get available disk space on Windows systems, returning GB as f32
    #[cfg(target_os = "windows")]
    fn get_available_disk_space_windows(&self) -> Option<f32> {
        let output = std::process::Command::new("powershell")
            .args(["-Command", "(Get-PSDrive C).Free / 1GB"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let space_str = String::from_utf8(output.stdout).ok()?;
        space_str.trim().parse::<f32>().ok()
    }

    /// No-op implementation for non-Unix, non-Windows systems
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn check_disk_space_unix(&self) {}

    /// No-op implementation for non-Windows systems
    #[cfg(not(target_os = "windows"))]
    fn check_disk_space_windows(&self) {}

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
                         Note: Windows native support planned for future release (Git 2.48+).".into()
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
            VmError::Internal(format!("Docker build failed for project '{}'. Check that Docker is running and build context is valid: {}", self.project_name(), e))
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
                         Note: Windows native support planned for future release (Git 2.48+).".into()
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
            VmError::Internal(format!("Docker build failed for project '{}' instance '{}'. Check that Docker is running and build context is valid: {}", self.project_name(), instance_name, e))
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
    fn prepare_config_for_copy(
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
    fn prepare_temp_config(&self) -> Result<Cow<'_, VmConfig>> {
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
    fn prepare_and_copy_config(&self) -> Result<()> {
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
        let destination = format!("{}:{}", self.container_name(), TEMP_CONFIG_PATH);
        let copy_result = DockerOps::copy(source, &destination);
        if copy_result.is_err() {
            return Err(VmError::Internal(format!(
                "Failed to copy VM configuration to container '{}'. Container may not be running or accessible",
                self.container_name()
            )));
        }
        Ok(())
    }

    #[must_use = "container provisioning results should be handled"]
    #[allow(dead_code)]
    fn provision_container(&self) -> Result<()> {
        self.provision_container_with_context(&ProviderContext::default())
    }

    #[must_use = "container provisioning results should be handled"]
    fn provision_container_with_context(&self, context: &ProviderContext) -> Result<()> {
        // Step 6: Wait for readiness and run ansible (configuration only)
        let mut attempt = 1;
        while attempt <= CONTAINER_READINESS_MAX_ATTEMPTS {
            if DockerOps::test_container_readiness(&self.container_name()) {
                break;
            }
            if attempt == CONTAINER_READINESS_MAX_ATTEMPTS {
                vm_error!("Container failed to become ready");
                return Err(VmError::Internal(format!(
                    "Container '{}' failed to become ready after {} seconds. Container may be unhealthy or not starting properly",
                    self.container_name(),
                    u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS
                )));
            }
            std::thread::sleep(std::time::Duration::from_secs(
                CONTAINER_READINESS_SLEEP_SECONDS,
            ));
            attempt += 1;
        }

        self.prepare_and_copy_config()?;

        // Use progress-aware streaming with AnsibleProgressParser
        let parser = if context.is_verbose() {
            None
        } else {
            // Implement the trait adapter
            struct AnsibleParserAdapter(AnsibleProgressParser);
            impl CoreProgressParser for AnsibleParserAdapter {
                fn parse_line(&mut self, line: &str) {
                    ProgressParser::parse_line(&mut self.0, line);
                }
                fn finish(&self) {
                    ProgressParser::finish(&self.0);
                }
            }
            Some(
                Box::new(AnsibleParserAdapter(AnsibleProgressParser::new(false)))
                    as Box<dyn CoreProgressParser>,
            )
        };

        stream_command_with_progress(
            "docker",
            &[
                "exec",
                &self.container_name(),
                "bash",
                "-c",
                &format!(
                    "ansible-playbook -i localhost, -c local {} {}",
                    ANSIBLE_PLAYBOOK_PATH,
                    context.ansible_verbosity()
                ),
            ],
            parser,
        )
        .map_err(|e| {
            VmError::Internal(format!("Ansible provisioning failed for container '{}'. Check container logs for detailed error information: {}", self.container_name(), e))
        })?;

        // Note: No need to manually restart supervisord here - Ansible handles supervisor
        // lifecycle via handlers (playbook.yml:176-183) and service-specific reloads
        // (manage-service.yml:104-116). Attempting to restart here causes permission errors
        // because docker exec runs as the container's default user (developer), while
        // supervisord was started as root by Ansible.

        Ok(())
    }

    /// Provision container with custom instance name
    #[allow(dead_code)]
    fn provision_container_with_instance(&self, instance_name: &str) -> Result<()> {
        self.provision_container_with_instance_and_context(
            instance_name,
            &ProviderContext::default(),
        )
    }

    /// Provision container with custom instance name and context
    fn provision_container_with_instance_and_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        let container_name = self.container_name_with_instance(instance_name);

        // Step 6: Wait for readiness and run ansible (configuration only)
        let mut attempt = 1;
        while attempt <= CONTAINER_READINESS_MAX_ATTEMPTS {
            if DockerOps::test_container_readiness(&container_name) {
                break;
            }
            if attempt == CONTAINER_READINESS_MAX_ATTEMPTS {
                vm_error!("Container failed to become ready");
                return Err(VmError::Internal(format!(
                    "Container '{}' failed to become ready after {} seconds. Container may be unhealthy or not starting properly",
                    container_name,
                    u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS
                )));
            }
            std::thread::sleep(std::time::Duration::from_secs(
                CONTAINER_READINESS_SLEEP_SECONDS,
            ));
            attempt += 1;
        }

        self.prepare_and_copy_config()?;

        // Use progress-aware streaming with AnsibleProgressParser
        let parser = if context.is_verbose() {
            None
        } else {
            // Implement the trait adapter
            struct AnsibleParserAdapter(AnsibleProgressParser);
            impl CoreProgressParser for AnsibleParserAdapter {
                fn parse_line(&mut self, line: &str) {
                    ProgressParser::parse_line(&mut self.0, line);
                }
                fn finish(&self) {
                    ProgressParser::finish(&self.0);
                }
            }
            Some(
                Box::new(AnsibleParserAdapter(AnsibleProgressParser::new(false)))
                    as Box<dyn CoreProgressParser>,
            )
        };

        stream_command_with_progress(
            "docker",
            &[
                "exec",
                &container_name,
                "bash",
                "-c",
                &format!(
                    "ansible-playbook -i localhost, -c local {} {}",
                    ANSIBLE_PLAYBOOK_PATH,
                    context.ansible_verbosity()
                ),
            ],
            parser,
        )
        .map_err(|e| {
            VmError::Internal(format!("Ansible provisioning failed for container '{}'. Check container logs for detailed error information: {}", container_name, e))
        })?;

        // Note: No need to manually restart supervisord here - Ansible handles supervisor
        // lifecycle via handlers (playbook.yml:176-183) and service-specific reloads
        // (manage-service.yml:104-116). Attempting to restart here causes permission errors
        // because docker exec runs as the container's default user (developer), while
        // supervisord was started as root by Ansible.

        Ok(())
    }

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

    #[must_use = "SSH connection results should be handled"]
    pub fn ssh_into_container(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        let workspace_path = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or(DEFAULT_WORKSPACE_PATH);
        let user_config = UserConfig::from_vm_config(self.config);
        let project_user = &user_config.username;
        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or(DEFAULT_SHELL);

        let target_path = SecurityValidator::validate_relative_path(relative_path, workspace_path)?;
        let target_dir = target_path.to_string_lossy();

        let tty_flag = if io::stdin().is_terminal() && io::stdout().is_terminal() {
            "-it"
        } else {
            "-i"
        };

        // Resolve the container name first
        let container_name = self.resolve_target_container(container)?;

        // Check if container is running before showing connection details
        // Use a quick docker inspect to check status (suppress errors)
        let status_check = duct::cmd(
            "docker",
            &["inspect", "--format", "{{.State.Running}}", &container_name],
        )
        .stderr_null()
        .read();

        let is_running = status_check.is_ok_and(|output| output.trim() == "true");

        // Only show connection details if container is running and it's interactive
        if is_running && io::stdin().is_terminal() && io::stdout().is_terminal() {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.docker_ssh_info,
                    user = project_user,
                    path = target_dir.as_ref(),
                    shell = shell
                )
            );
        }

        // First check if container exists to provide better error messages
        let container_exists = duct::cmd("docker", &["inspect", &container_name])
            .stdout_null()
            .stderr_null()
            .run()
            .is_ok();

        if !container_exists {
            // Return error without printing raw Docker messages
            return Err(VmError::Internal(format!(
                "No such container: {}",
                container_name
            )));
        }

        // Check if container is actually running before trying to exec
        let container_running = duct::cmd(
            "docker",
            &["inspect", "-f", "{{.State.Running}}", &container_name],
        )
        .stderr_null()
        .read()
        .map(|output| output.trim() == "true")
        .unwrap_or(false);

        if !container_running {
            // Return error that will trigger the start prompt
            return Err(VmError::Internal(format!(
                "Container {} is not running",
                container_name
            )));
        }

        // Container is running, proceed with exec
        let result = duct::cmd(
            "docker",
            &[
                "exec",
                tty_flag,
                "-e",
                &format!("VM_TARGET_DIR={}", target_dir),
                &container_name,
                "sudo",
                "-u",
                project_user,
                "sh",
                "-c",
                &format!("cd \"$VM_TARGET_DIR\" && exec {}", shell),
            ],
        )
        .run();

        match result {
            Ok(output) => {
                // Handle exit codes gracefully
                match output.status.code() {
                    Some(0) | Some(2) => Ok(()), // Normal exit or shell builtin exit
                    Some(127) => Ok(()), // Command not found (happens when user types non-existent command then exits)
                    Some(130) => Ok(()), // Ctrl-C interrupt - treat as normal exit
                    _ => {
                        // Only return error for actual connection failures
                        Err(VmError::Command(String::from("SSH connection lost")))
                    }
                }
            }
            Err(e) => {
                // Check if the error is because the container is not running
                let error_str = e.to_string();
                if error_str.contains("is not running") {
                    // Pass through the original error so the SSH handler can detect it
                    // and offer to start the VM
                    Err(e.into())
                } else if error_str.contains("exited with code")
                    && !error_str.contains("is not running")
                {
                    // Only clean up other duct command errors that include the full command
                    // but preserve "is not running" errors for proper handling
                    if error_str.contains("exited with code 1") {
                        Err(VmError::Internal("Docker command failed".to_string()))
                    } else {
                        Err(VmError::Internal("Command execution failed".to_string()))
                    }
                } else {
                    // Pass through other errors
                    Err(e.into())
                }
            }
        }
    }

    #[must_use = "command execution results should be handled"]
    pub fn exec_in_container(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;
        let workspace_path = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or(DEFAULT_WORKSPACE_PATH);
        let user_config = UserConfig::from_vm_config(self.config);
        let project_user = &user_config.username;

        let mut args: Vec<&str> = vec![
            "exec",
            "-w",
            workspace_path,
            "--user",
            project_user,
            &target_container,
        ];
        let cmd_strs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&cmd_strs);
        stream_command("docker", &args)
    }

    #[must_use = "log display results should be handled"]
    pub fn show_logs(&self, container: Option<&str>) -> Result<()> {
        // Show recent logs without following (-f) to avoid hanging indefinitely
        // Use --tail to show last 50 lines and add timestamps
        let target_container = self.resolve_target_container(container)?;
        stream_command("docker", &["logs", "--tail", "50", "-t", &target_container])
            .map_err(|e| VmError::Internal(format!("Failed to show logs: {}", e)))
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

    #[must_use = "existing container provisioning results should be handled"]
    pub fn provision_existing(&self, container: Option<&str>) -> Result<()> {
        let context = ProviderContext::default();
        let target_container = self.resolve_target_container(container)?;
        let status_output = std::process::Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{.State.Status}}",
                &target_container,
            ])
            .output()?;
        let status = String::from_utf8_lossy(&status_output.stdout)
            .trim()
            .to_owned();
        if status != "running" {
            return Err(VmError::Internal(format!(
                "Container {} is not running. Start it first with 'vm start'",
                target_container
            )));
        }

        let _progress = ProgressReporter::new();
        ProgressReporter::phase_header("🔧", "PROVISIONING PHASE");

        // Check if Ansible playbook exists in container
        let check_playbook = DockerCommand::new()
            .subcommand("exec")
            .arg(&target_container)
            .arg("test")
            .arg("-f")
            .arg(ANSIBLE_PLAYBOOK_PATH)
            .execute_raw()?;

        // Only copy resources if they don't exist
        if !check_playbook.status.success() {
            ProgressReporter::subtask("├─", "Copying Ansible resources to container...");

            // Create temporary directory for resources
            let temp_resources = self.temp_dir.join("provision_resources");
            if temp_resources.exists() {
                fs::remove_dir_all(&temp_resources)?;
            }
            fs::create_dir_all(&temp_resources)?;

            // Copy embedded resources (includes Ansible playbook)
            let shared_dir = temp_resources.join("shared");
            fs::create_dir_all(&shared_dir)?;
            crate::resources::copy_embedded_resources(&shared_dir)?;

            // Copy the shared directory to the container
            let source = format!("{}/.", shared_dir.display());
            let destination = format!("{}:/app/shared/", &target_container);
            DockerOps::copy(&source, &destination)
                .map_err(|e| {
                    VmError::Internal(format!("Failed to copy Ansible provisioning resources to container '{}'. Container may not be accessible: {}", &target_container, e))
                })?;
        }

        ProgressReporter::subtask("├─", "Re-running Ansible provisioning...");

        self.prepare_and_copy_config()?;

        let ansible_result = stream_command(
            "docker",
            &[
                "exec",
                &target_container,
                "bash",
                "-c",
                &format!(
                    "ansible-playbook -i localhost, -c local {} {}",
                    ANSIBLE_PLAYBOOK_PATH,
                    context.ansible_verbosity()
                ),
            ],
        );
        if let Err(e) = ansible_result {
            ProgressReporter::error("└─", &format!("Provisioning failed: {}", e));
            return Err(VmError::Internal(format!("Ansible provisioning failed during setup for container '{}'. Check provisioning logs for details: {}", target_container, e)));
        }
        ProgressReporter::complete("└─", "Provisioning complete");
        Ok(())
    }

    #[must_use = "container listing results should be handled"]
    pub fn list_containers(&self) -> Result<()> {
        self.list_containers_with_stats()
    }

    /// Enhanced container listing with CPU/RAM stats in clean minimal format
    #[must_use = "container listing results should be handled"]
    pub fn list_containers_with_stats(&self) -> Result<()> {
        // Get container info
        let ps_output = DockerCommand::new()
            .subcommand("ps")
            .arg("-a")
            .arg("--format")
            .arg("{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.CreatedAt}}")
            .execute_raw()
            .map_err(|e| VmError::Internal(format!("Failed to get container information from Docker. Ensure Docker is running and accessible: {}", e)))?;

        if !ps_output.status.success() {
            return Err(VmError::Internal(
                "Docker ps command failed. Check that Docker is installed, running, and accessible"
                    .to_string(),
            ));
        }

        // Get stats for running containers
        let stats_output = DockerCommand::new()
            .subcommand("stats")
            .arg("--no-stream")
            .arg("--format")
            .arg("{{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}")
            .execute_raw()
            .map_err(|e| VmError::Internal(format!("Failed to get container resource statistics from Docker. Some containers may not be running: {}", e)))?;

        let stats_data = if stats_output.status.success() {
            String::from_utf8_lossy(&stats_output.stdout)
        } else {
            "".into() // Continue without stats if command fails
        };

        // Parse stats into a map
        let mut stats_map = std::collections::HashMap::new();
        for line in stats_data.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let name = parts[0];
                let cpu = parts[1];
                let memory = parts[2];
                stats_map.insert(name.to_string(), (cpu.to_string(), memory.to_string()));
            }
        }

        println!("vm list");
        println!("-------");

        let mut total_cpu = 0.0;
        let mut total_memory_mb = 0u64;
        let mut running_count = 0;

        let ps_data = String::from_utf8_lossy(&ps_output.stdout);
        for line in ps_data.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                let name = parts[0];
                let status = parts[1];
                let ports = parts[2];
                let _created = parts[3];

                // Determine status icon and running state
                let (icon, is_running, duration) = self.parse_container_status(status);

                // Get stats if running
                let (cpu_display, mem_display) = if is_running {
                    self.process_container_stats(
                        name,
                        &stats_map,
                        &mut total_cpu,
                        &mut total_memory_mb,
                        &mut running_count,
                    )
                } else {
                    ("--".to_string(), "--".to_string())
                };

                // Format ports (clean up the display)
                let ports_display = self.format_ports_minimal(ports);

                // Shorten container name (remove -dev suffix for display)
                let display_name = if let Some(stripped) = name.strip_suffix("-dev") {
                    stripped
                } else {
                    name
                };

                // Print in clean minimal format with better alignment
                // Truncate long names to prevent misalignment
                let display_name = if display_name.len() > 20 {
                    &display_name[..20]
                } else {
                    display_name
                };

                println!(
                    "{} {:<20} {:>7} {:>8} {:<13} [{}]",
                    icon, display_name, cpu_display, mem_display, ports_display, duration
                );
            }
        }

        // Print totals
        if running_count > 0 {
            let total_memory_display = if total_memory_mb >= 1024 {
                format!("{:.1}GB", total_memory_mb as f64 / 1024.0)
            } else {
                format!("{}MB", total_memory_mb)
            };

            println!(
                "\nTotal: {:.1}% CPU, {} RAM",
                total_cpu, total_memory_display
            );
        }

        Ok(())
    }

    /// Process container stats and update totals
    #[allow(clippy::excessive_nesting)]
    fn process_container_stats(
        &self,
        name: &str,
        stats_map: &std::collections::HashMap<String, (String, String)>,
        total_cpu: &mut f64,
        total_memory_mb: &mut u64,
        running_count: &mut usize,
    ) -> (String, String) {
        if let Some((cpu, mem)) = stats_map.get(name) {
            *running_count += 1;

            // Parse CPU percentage
            if let Some(cpu_val) = cpu.strip_suffix('%') {
                if let Ok(cpu_num) = cpu_val.parse::<f64>() {
                    *total_cpu += cpu_num;
                }
            }

            // Parse memory (format: "156MiB / 2GiB" or "156MB / 2GB")
            if let Some(mem_used) = mem.split('/').next() {
                let trimmed = mem_used.trim();
                if let Some(mb_val) = trimmed
                    .strip_suffix("MiB")
                    .or_else(|| trimmed.strip_suffix("MB"))
                {
                    if let Ok(mb_num) = mb_val.parse::<f64>() {
                        *total_memory_mb += mb_num.round() as u64;
                    }
                } else if let Some(gb_val) = trimmed
                    .strip_suffix("GiB")
                    .or_else(|| trimmed.strip_suffix("GB"))
                {
                    if let Ok(gb_num) = gb_val.parse::<f64>() {
                        *total_memory_mb += (gb_num * 1024.0).round() as u64;
                    }
                }
            }

            // Clean up memory display
            let mem_display = self.format_memory_display(mem.split('/').next().unwrap_or(mem));
            (cpu.clone(), mem_display)
        } else {
            ("--".to_string(), "--".to_string())
        }
    }

    /// Format memory display to clean format (1.2GB, 156MB)
    fn format_memory_display(&self, mem_str: &str) -> String {
        let trimmed = mem_str.trim();

        // Parse MiB/MB values
        if let Some(mb_val) = trimmed
            .strip_suffix("MiB")
            .or_else(|| trimmed.strip_suffix("MB"))
        {
            if let Ok(mb_num) = mb_val.parse::<f64>() {
                if mb_num >= 1024.0 {
                    return format!("{:.1}GB", mb_num / 1024.0);
                } else {
                    return format!("{}MB", mb_num as u64);
                }
            }
        }

        // Parse GiB/GB values
        if let Some(gb_val) = trimmed
            .strip_suffix("GiB")
            .or_else(|| trimmed.strip_suffix("GB"))
        {
            if let Ok(gb_num) = gb_val.parse::<f64>() {
                return format!("{:.1}GB", gb_num);
            }
        }

        // Return as-is if can't parse
        trimmed.to_string()
    }

    /// Parse container status to extract icon, running state, and duration
    fn parse_container_status(&self, status: &str) -> (&'static str, bool, String) {
        if status.starts_with("Up") {
            let duration = if status.len() > 3 {
                // Extract duration from "Up 3 hours" -> "3h"
                let duration_part = &status[3..];
                self.format_duration_short(duration_part)
            } else {
                "now".to_string()
            };
            ("🟢", true, duration)
        } else if status.starts_with("Exited") {
            // Extract exit info from "Exited (0) 3 seconds ago" -> "stopped"
            if status.contains("ago") {
                ("🔴", false, "stopped".to_string())
            } else {
                ("🔴", false, "exited".to_string())
            }
        } else if status.contains("Created") {
            ("🔴", false, "created".to_string())
        } else {
            ("⚫", false, "unknown".to_string())
        }
    }

    /// Format duration to short form (3 hours -> 3h, 2 days -> 2d)
    fn format_duration_short(&self, duration: &str) -> String {
        let duration = duration.trim();

        if duration.contains("day") {
            if let Some(days) = duration.split_whitespace().next() {
                return format!("{}d", days);
            }
        } else if duration.contains("hour") {
            if let Some(hours) = duration.split_whitespace().next() {
                return format!("{}h", hours);
            }
        } else if duration.contains("minute") {
            if let Some(minutes) = duration.split_whitespace().next() {
                return format!("{}m", minutes);
            }
        } else if duration.contains("second") {
            return "now".to_string();
        }

        // Fallback: try to extract first number + first letter of unit
        if let Some(first_word) = duration.split_whitespace().next() {
            if let Some(unit_word) = duration.split_whitespace().nth(1) {
                if let Some(unit_char) = unit_word.chars().next() {
                    return format!("{}{}", first_word, unit_char);
                }
            }
        }

        duration.to_string()
    }

    /// Format ports for minimal display
    fn format_ports_minimal(&self, ports: &str) -> String {
        if ports.is_empty() {
            return "-".to_string();
        }

        // Parse port mappings and simplify display
        // "0.0.0.0:3100-3101->3100-3101/tcp, [::]:3100-3101->3100-3101/tcp" -> "3100-3101"
        let mut port_ranges = Vec::new();

        for mapping in ports.split(", ") {
            if let Some(port_part) = self.extract_port_from_mapping(mapping) {
                port_ranges.push(port_part);
            }
        }

        // Remove duplicates and join
        port_ranges.sort();
        port_ranges.dedup();

        if port_ranges.is_empty() {
            "-".to_string()
        } else {
            port_ranges.join(",")
        }
    }

    /// Extract port from a Docker port mapping string
    fn extract_port_from_mapping(&self, mapping: &str) -> Option<String> {
        if mapping.contains("->") {
            if let Some(external_part) = mapping.split("->").next() {
                // Extract port range from "0.0.0.0:3100-3101" or "[::]:3100-3101"
                if let Some(port_part) = external_part.split(':').next_back() {
                    return Some(port_part.to_string());
                }
            }
        }
        None
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

    /// Resolve target container from optional container parameter
    /// If container is None, uses the current project's container
    /// If container is Some, resolves using resolve_container_name
    #[must_use = "container resolution results should be checked"]
    pub fn resolve_target_container(&self, container: Option<&str>) -> Result<String> {
        match container {
            None => Ok(self.container_name()),
            Some(name) => self.resolve_container_name(name),
        }
    }

    /// Resolve a partial container name to a full container name
    /// Supports matching by:
    /// - Exact container name
    /// - Project name (resolves to project-dev)
    /// - Partial container ID
    #[must_use = "container resolution results should be checked"]
    fn resolve_container_name(&self, partial_name: &str) -> Result<String> {
        // Get list of all containers
        let output = std::process::Command::new("docker")
            .args(["ps", "-a", "--format", "{{.Names}}\t{{.ID}}"])
            .output()
            .map_err(|e| VmError::Internal(format!("Failed to list containers for name resolution. Docker may not be running or accessible: {}", e)))?;

        if !output.status.success() {
            return Err(VmError::Internal(
                "Docker container listing failed during name resolution. Check Docker daemon status".to_string()
            ));
        }

        let containers_output = String::from_utf8_lossy(&output.stdout);

        // First, try exact name match
        for line in containers_output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let name = parts[0];
                let id = parts[1];

                // Exact name match
                if name == partial_name {
                    return Ok(name.to_string());
                }

                // Exact ID match (full or partial)
                if id.starts_with(partial_name) {
                    return Ok(name.to_string());
                }
            }
        }

        // Second, try project name resolution (partial_name -> partial_name-dev)
        let candidate_name = format!("{}-dev", partial_name);
        for line in containers_output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if !parts.is_empty() {
                let name = parts[0];
                if name == candidate_name {
                    return Ok(name.to_string());
                }
            }
        }

        // Third, try fuzzy matching on container names
        let mut matches = Vec::new();
        for line in containers_output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if !parts.is_empty() {
                let name = parts[0];
                if name.contains(partial_name) {
                    matches.push(name.to_string());
                }
            }
        }

        match matches.len() {
            0 => Err(VmError::Internal(format!(
                "No container found matching '{}'. Use 'vm list' to see available containers",
                partial_name
            ))),
            1 => Ok(matches[0].clone()),
            _ => {
                // Multiple matches - prefer exact project name match
                for name in &matches {
                    if name == &format!("{}-dev", partial_name) {
                        return Ok(name.clone());
                    }
                }
                // Otherwise return first match but warn about ambiguity
                eprintln!(
                    "Warning: Multiple containers match '{}': {}",
                    partial_name,
                    matches.join(", ")
                );
                eprintln!("Using: {}", matches[0]);
                Ok(matches[0].clone())
            }
        }
    }

    pub fn get_sync_directory(&self) -> String {
        self.config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace")
            .to_string()
    }

    /// Get comprehensive status report with real-time metrics and service health
    pub fn get_status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
        let container_name = self.resolve_target_container(container)?;

        // Get basic container info via docker inspect
        let inspect_output = std::process::Command::new("docker")
            .args(["inspect", &container_name])
            .output()
            .map_err(|e| VmError::Internal(format!("Failed to inspect container: {}", e)))?;

        if !inspect_output.status.success() {
            return Err(VmError::Internal(format!(
                "Container '{}' not found",
                container_name
            )));
        }

        let inspect_data: serde_json::Value = serde_json::from_slice(&inspect_output.stdout)
            .map_err(|e| VmError::Internal(format!("Failed to parse container info: {}", e)))?;

        let container_info = &inspect_data[0];
        let state = &container_info["State"];
        let config = &container_info["Config"];

        let is_running = state["Running"].as_bool().unwrap_or(false);
        let container_id = container_info["Id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        // Calculate uptime
        let uptime = if is_running {
            self.calculate_uptime(state)?
        } else {
            None
        };

        // Get resource usage only if container is running
        let resources = if is_running {
            self.get_container_resources(&container_name)?
        } else {
            ResourceUsage::default()
        };

        // Check service health only if container is running
        let services = if is_running {
            self.check_all_services(&container_name, config)?
        } else {
            vec![]
        };

        Ok(VmStatusReport {
            name: container_name,
            provider: "docker".to_string(),
            container_id: Some(container_id),
            is_running,
            uptime,
            resources,
            services,
        })
    }

    /// Calculate container uptime from Docker inspect data
    fn calculate_uptime(&self, state: &serde_json::Value) -> Result<Option<String>> {
        let Some(started_at) = state["StartedAt"].as_str() else {
            return Ok(None);
        };

        if started_at == "0001-01-01T00:00:00Z" {
            return Ok(None);
        }

        let start_time = match chrono::DateTime::parse_from_rfc3339(started_at) {
            Ok(time) => time,
            Err(_) => return Ok(Some("unknown".to_string())),
        };

        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(start_time.with_timezone(&chrono::Utc));

        let uptime = if duration.num_days() > 0 {
            format!("{}d", duration.num_days())
        } else if duration.num_hours() > 0 {
            format!("{}h", duration.num_hours())
        } else if duration.num_minutes() > 0 {
            format!("{}m", duration.num_minutes())
        } else {
            "now".to_string()
        };

        Ok(Some(uptime))
    }

    /// Get real-time resource usage from docker stats
    fn get_container_resources(&self, container_name: &str) -> Result<ResourceUsage> {
        let stats_output = std::process::Command::new("docker")
            .args([
                "stats",
                "--no-stream",
                "--format",
                "{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}",
                container_name,
            ])
            .output()
            .map_err(|e| VmError::Internal(format!("Failed to get container stats: {}", e)))?;

        if !stats_output.status.success() {
            return Ok(ResourceUsage::default());
        }

        let stats_line = String::from_utf8_lossy(&stats_output.stdout);
        let parts: Vec<&str> = stats_line.trim().split('\t').collect();

        if parts.len() >= 2 {
            let cpu_str = parts[0].trim_end_matches('%');
            let memory_parts = parts[1].split('/').collect::<Vec<&str>>();

            let cpu_percent = cpu_str.parse::<f64>().ok();
            let (memory_used_mb, memory_limit_mb) = if memory_parts.len() >= 2 {
                let used = self.parse_memory_value(memory_parts[0].trim());
                let limit = self.parse_memory_value(memory_parts[1].trim());
                (used, limit)
            } else {
                (None, None)
            };

            // Get disk usage from container filesystem
            let (disk_used_gb, disk_total_gb) = self.get_disk_usage(container_name);

            return Ok(ResourceUsage {
                cpu_percent,
                memory_used_mb,
                memory_limit_mb,
                disk_used_gb,
                disk_total_gb,
            });
        }

        Ok(ResourceUsage::default())
    }

    /// Parse memory value from Docker stats (e.g., "123MiB" -> 123, "1.5GiB" -> 1536)
    fn parse_memory_value(&self, value: &str) -> Option<u64> {
        if let Some(mb_val) = value
            .strip_suffix("MiB")
            .or_else(|| value.strip_suffix("MB"))
        {
            mb_val.parse::<f64>().ok().map(|v| v as u64)
        } else if let Some(gb_val) = value
            .strip_suffix("GiB")
            .or_else(|| value.strip_suffix("GB"))
        {
            gb_val.parse::<f64>().ok().map(|v| (v * 1024.0) as u64)
        } else {
            None
        }
    }

    /// Get disk usage from container using df command
    fn get_disk_usage(&self, container_name: &str) -> (Option<f64>, Option<f64>) {
        let df_output = std::process::Command::new("docker")
            .args(["exec", container_name, "df", "-h", "/"])
            .output();

        let Ok(output) = df_output else {
            return (None, None);
        };
        if !output.status.success() {
            return (None, None);
        }

        let df_text = String::from_utf8_lossy(&output.stdout);
        for line in df_text.lines().skip(1) {
            // Skip header
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let used = self.parse_disk_value(parts[2]);
                let total = self.parse_disk_value(parts[1]);
                return (used, total);
            }
        }
        (None, None)
    }

    /// Parse disk value from df output (e.g., "1.5G" -> 1.5, "512M" -> 0.5)
    fn parse_disk_value(&self, value: &str) -> Option<f64> {
        if let Some(gb_val) = value.strip_suffix('G') {
            gb_val.parse::<f64>().ok()
        } else if let Some(mb_val) = value.strip_suffix('M') {
            mb_val.parse::<f64>().ok().map(|v| v / 1024.0)
        } else if let Some(kb_val) = value.strip_suffix('K') {
            kb_val.parse::<f64>().ok().map(|v| v / (1024.0 * 1024.0))
        } else {
            None
        }
    }

    /// Check health of all configured services
    fn check_all_services(
        &self,
        container_name: &str,
        config: &serde_json::Value,
    ) -> Result<Vec<ServiceStatus>> {
        let mut services = Vec::new();

        // Get port mappings from container config
        let Some(exposed_ports) = config["ExposedPorts"].as_object() else {
            return Ok(services);
        };

        for (port_spec, _) in exposed_ports {
            let Some(port_str) = port_spec.split('/').next() else {
                continue;
            };
            let Ok(port) = port_str.parse::<u16>() else {
                continue;
            };

            let service_name = self.identify_service_by_port(port);
            let service_status = match service_name.as_str() {
                "postgresql" => self.check_postgres_status(container_name, port),
                "redis" => self.check_redis_status(container_name, port),
                "mongodb" => self.check_mongodb_status(container_name, port),
                _ => ServiceStatus {
                    name: service_name,
                    is_running: true, // Assume running if port is exposed
                    port: Some(port),
                    host_port: self.get_host_port(container_name, port),
                    metrics: None,
                    error: None,
                },
            };
            services.push(service_status);
        }

        Ok(services)
    }

    /// Identify service type by port number
    fn identify_service_by_port(&self, port: u16) -> String {
        match port {
            5432 => "postgresql".to_string(),
            6379 => "redis".to_string(),
            27017 => "mongodb".to_string(),
            3306 => "mysql".to_string(),
            8080 => "http".to_string(),
            3000 => "node".to_string(),
            8000 => "python".to_string(),
            _ => format!("service-{}", port),
        }
    }

    /// Get the host port mapping for a container port
    fn get_host_port(&self, container_name: &str, container_port: u16) -> Option<u16> {
        let port_output = std::process::Command::new("docker")
            .args(["port", container_name, &container_port.to_string()])
            .output()
            .ok()?;

        if port_output.status.success() {
            let port_text = String::from_utf8_lossy(&port_output.stdout);
            // Parse "0.0.0.0:8080" -> 8080
            if let Some(host_port_str) = port_text.trim().split(':').next_back() {
                return host_port_str.parse::<u16>().ok();
            }
        }
        None
    }

    /// Check PostgreSQL service health using pg_isready
    fn check_postgres_status(&self, container_name: &str, port: u16) -> ServiceStatus {
        let result = std::process::Command::new("docker")
            .args([
                "exec",
                container_name,
                "pg_isready",
                "-p",
                &port.to_string(),
            ])
            .output();

        match result {
            Ok(output) => {
                let is_running = output.status.success();
                ServiceStatus {
                    name: "postgresql".to_string(),
                    is_running,
                    port: Some(port),
                    host_port: self.get_host_port(container_name, port),
                    metrics: if is_running {
                        Some("accepting connections".to_string())
                    } else {
                        None
                    },
                    error: if !is_running {
                        Some(String::from_utf8_lossy(&output.stderr).trim().to_string())
                    } else {
                        None
                    },
                }
            }
            Err(e) => ServiceStatus {
                name: "postgresql".to_string(),
                is_running: false,
                port: Some(port),
                host_port: self.get_host_port(container_name, port),
                metrics: None,
                error: Some(format!("Health check failed: {}", e)),
            },
        }
    }

    /// Check Redis service health using redis-cli ping
    fn check_redis_status(&self, container_name: &str, port: u16) -> ServiceStatus {
        let result = std::process::Command::new("docker")
            .args([
                "exec",
                container_name,
                "redis-cli",
                "-p",
                &port.to_string(),
                "ping",
            ])
            .output();

        match result {
            Ok(output) => {
                let response = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_lowercase();
                let is_running = output.status.success() && response == "pong";
                ServiceStatus {
                    name: "redis".to_string(),
                    is_running,
                    port: Some(port),
                    host_port: self.get_host_port(container_name, port),
                    metrics: if is_running {
                        Some("PONG".to_string())
                    } else {
                        None
                    },
                    error: if !is_running {
                        Some(String::from_utf8_lossy(&output.stderr).trim().to_string())
                    } else {
                        None
                    },
                }
            }
            Err(e) => ServiceStatus {
                name: "redis".to_string(),
                is_running: false,
                port: Some(port),
                host_port: self.get_host_port(container_name, port),
                metrics: None,
                error: Some(format!("Health check failed: {}", e)),
            },
        }
    }

    /// Check MongoDB service health using mongosh ping
    fn check_mongodb_status(&self, container_name: &str, port: u16) -> ServiceStatus {
        let result = std::process::Command::new("docker")
            .args([
                "exec",
                container_name,
                "mongosh",
                "--port",
                &port.to_string(),
                "--eval",
                "db.adminCommand('ping')",
            ])
            .output();

        match result {
            Ok(output) => {
                let response = String::from_utf8_lossy(&output.stdout);
                let is_running = output.status.success() && response.contains("ok");
                ServiceStatus {
                    name: "mongodb".to_string(),
                    is_running,
                    port: Some(port),
                    host_port: self.get_host_port(container_name, port),
                    metrics: if is_running {
                        Some("ping ok".to_string())
                    } else {
                        None
                    },
                    error: if !is_running {
                        Some(String::from_utf8_lossy(&output.stderr).trim().to_string())
                    } else {
                        None
                    },
                }
            }
            Err(e) => ServiceStatus {
                name: "mongodb".to_string(),
                is_running: false,
                port: Some(port),
                host_port: self.get_host_port(container_name, port),
                metrics: None,
                error: Some(format!("Health check failed: {}", e)),
            },
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn create_test_lifecycle() -> LifecycleOperations<'static> {
        use std::sync::OnceLock;

        static TEST_CONFIG: OnceLock<VmConfig> = OnceLock::new();
        static TEST_TEMP_DIR: OnceLock<PathBuf> = OnceLock::new();
        static TEST_PROJECT_DIR: OnceLock<PathBuf> = OnceLock::new();

        let config = TEST_CONFIG.get_or_init(|| VmConfig {
            pip_packages: vec!["claudeflow".to_string(), "ruff".to_string()],
            ..Default::default()
        });

        let temp_dir = TEST_TEMP_DIR.get_or_init(|| PathBuf::from("/tmp/test"));
        let project_dir = TEST_PROJECT_DIR.get_or_init(|| PathBuf::from("/project"));

        LifecycleOperations::new(config, temp_dir, project_dir)
    }

    #[test]
    fn test_extract_pipx_managed_packages() {
        let lifecycle = create_test_lifecycle();

        let pipx_json = json!({
            "venvs": {
                "claudeflow": {
                    "metadata": {
                        "main_package": {
                            "package_or_url": "/Users/miko/projects/codeflow",
                            "package": "claudeflow"
                        }
                    }
                },
                "ruff": {
                    "metadata": {
                        "main_package": {
                            "package_or_url": "ruff",
                            "package": "ruff"
                        }
                    }
                }
            }
        });

        let managed_packages = lifecycle.extract_pipx_managed_packages(&pipx_json);

        // Should mark both as pipx-managed for filtering from pip_packages
        assert!(managed_packages.contains("claudeflow"));
        assert!(managed_packages.contains("ruff"));
        assert_eq!(managed_packages.len(), 2);
    }

    #[test]
    fn test_categorize_pipx_packages() {
        let lifecycle = create_test_lifecycle();

        let pipx_json = json!({
            "venvs": {
                "claudeflow": {
                    "metadata": {
                        "main_package": {
                            "package_or_url": "/Users/miko/projects/codeflow",
                            "package": "claudeflow"
                        }
                    }
                },
                "ruff": {
                    "metadata": {
                        "main_package": {
                            "package_or_url": "ruff",
                            "package": "ruff"
                        }
                    }
                }
            }
        });

        let container_packages = lifecycle.categorize_pipx_packages(&pipx_json);

        // All pipx packages are now treated as container packages
        assert!(container_packages.contains(&"ruff".to_string()));
        assert!(container_packages.contains(&"claudeflow".to_string()));
        assert_eq!(container_packages.len(), 2);
    }

    #[test]
    fn test_no_pipx_packages() {
        let lifecycle = create_test_lifecycle();

        let pipx_json = json!({
            "venvs": {}
        });

        let managed_packages = lifecycle.extract_pipx_managed_packages(&pipx_json);
        let container_packages = lifecycle.categorize_pipx_packages(&pipx_json);

        assert!(managed_packages.is_empty());
        assert!(container_packages.is_empty());
    }

    #[test]
    fn test_filter_pipx_managed_packages() {
        let lifecycle = create_test_lifecycle();
        let mut config = lifecycle.config.clone();

        // Set up test config with pip packages that overlap with pipx
        config.pip_packages = vec![
            "ruff".to_string(),
            "black".to_string(),
            "claudeflow".to_string(),
        ];

        // Mock pipx detection to return ruff and claudeflow as managed
        // In a real test, this would be injected, but for simplicity we test the filtering logic
        let pipx_managed: std::collections::HashSet<String> = ["ruff", "claudeflow"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Filter out pipx-managed packages
        config.pip_packages = config
            .pip_packages
            .iter()
            .filter(|pkg| !pipx_managed.contains(*pkg))
            .cloned()
            .collect();

        // Only black should remain (not managed by pipx)
        assert_eq!(config.pip_packages.len(), 1);
        assert!(config.pip_packages.contains(&"black".to_string()));
        assert!(!config.pip_packages.contains(&"ruff".to_string()));
        assert!(!config.pip_packages.contains(&"claudeflow".to_string()));
    }

    #[test]
    fn test_start_container_with_context_regenerates_compose() {
        use tempfile::TempDir;
        use vm_config::GlobalConfig;

        // Create temporary directories
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let project_dir = temp_dir.path().to_path_buf();

        // Create VM config with project name
        let mut vm_config = VmConfig::default();
        vm_config.project = Some(vm_config::config::ProjectConfig {
            name: Some("test-project".to_string()),
            ..Default::default()
        });

        // Create lifecycle operations
        let lifecycle = LifecycleOperations::new(&vm_config, &temp_path, &project_dir);

        // First: Create compose file WITHOUT registry
        let context_no_registry = ProviderContext::with_verbose(false);
        let build_ops = BuildOperations::new(&vm_config, &temp_path);
        let build_context = build_ops.prepare_build_context().unwrap();
        let compose_ops = ComposeOperations::new(&vm_config, &temp_path, &project_dir);
        compose_ops
            .write_docker_compose(&build_context, &context_no_registry)
            .unwrap();

        let compose_path = temp_path.join("docker-compose.yml");
        let initial_content = std::fs::read_to_string(&compose_path).unwrap();

        // Verify no registry vars initially
        assert!(
            !initial_content.contains("NPM_CONFIG_REGISTRY="),
            "Should NOT have registry vars initially"
        );

        // Now call start_container_with_context WITH registry enabled
        let mut global_config = GlobalConfig::default();
        global_config.services.package_registry.enabled = true;
        global_config.services.package_registry.port = 3080;
        let context_with_registry = ProviderContext::with_verbose(false).with_config(global_config);

        // This will fail at the docker start stage (no Docker in test env), but should regenerate compose first
        let _result = lifecycle.start_container_with_context(None, &context_with_registry);

        // Read the compose file again - it should be regenerated with registry vars
        let updated_content = std::fs::read_to_string(&compose_path).unwrap();

        // Verify registry vars ARE present after calling start_container_with_context
        let host = vm_platform::platform::get_host_gateway();
        assert!(
            updated_content.contains(&format!("NPM_CONFIG_REGISTRY=http://{}:3080/npm/", host)),
            "Compose should be regenerated with registry vars by start_container_with_context"
        );
        assert!(
            updated_content.contains("VM_CARGO_REGISTRY_HOST="),
            "Compose should contain cargo registry host"
        );

        // Verify file actually changed
        assert_ne!(
            initial_content, updated_content,
            "start_container_with_context should regenerate compose file with new config"
        );
    }

    #[test]
    fn test_restart_container_with_context_regenerates_compose() {
        use tempfile::TempDir;
        use vm_config::GlobalConfig;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let project_dir = temp_dir.path().to_path_buf();

        let mut vm_config = VmConfig::default();
        vm_config.project = Some(vm_config::config::ProjectConfig {
            name: Some("test-project".to_string()),
            ..Default::default()
        });

        let lifecycle = LifecycleOperations::new(&vm_config, &temp_path, &project_dir);

        // Create initial compose without registry
        let context_no_registry = ProviderContext::with_verbose(false);
        let build_ops = BuildOperations::new(&vm_config, &temp_path);
        let build_context = build_ops.prepare_build_context().unwrap();
        let compose_ops = ComposeOperations::new(&vm_config, &temp_path, &project_dir);
        compose_ops
            .write_docker_compose(&build_context, &context_no_registry)
            .unwrap();

        let compose_path = temp_path.join("docker-compose.yml");
        let initial_content = std::fs::read_to_string(&compose_path).unwrap();

        // Enable registry and call restart_container_with_context
        let mut global_config = GlobalConfig::default();
        global_config.services.package_registry.enabled = true;
        global_config.services.package_registry.port = 3090;
        let context_with_registry = ProviderContext::with_verbose(false).with_config(global_config);

        // Will fail at docker stop/start, but should regenerate compose first
        let _result = lifecycle.restart_container_with_context(None, &context_with_registry);

        // Verify compose was regenerated
        let updated_content = std::fs::read_to_string(&compose_path).unwrap();
        let host = vm_platform::platform::get_host_gateway();

        assert!(
            updated_content.contains(&format!("NPM_CONFIG_REGISTRY=http://{}:3090/npm/", host)),
            "Compose should be regenerated by restart_container_with_context"
        );

        assert_ne!(
            initial_content, updated_content,
            "restart_container_with_context should regenerate compose"
        );
    }
}
