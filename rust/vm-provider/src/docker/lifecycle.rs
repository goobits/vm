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
use anyhow::{Context, Result};
use is_terminal::IsTerminal;
use serde_json::Value;

// Internal imports
use super::{
    build::BuildOperations, command::DockerCommand, compose::ComposeOperations, ComposeCommand,
    DockerOps, UserConfig,
};
use crate::{
    audio::MacOSAudioManager, error::ProviderError, progress::ProgressReporter,
    security::SecurityValidator, TempProvider, TempVmState,
};
use vm_common::command_stream::stream_command;
use vm_common::errors;
use vm_common::vm_error_with_details;
use vm_common::{vm_dbg, vm_error, vm_error_hint, vm_success, vm_warning};
use vm_config::config::VmConfig;

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
                .with_context(|| "Failed to parse pipx package listing output as JSON. pipx may have returned invalid output")?;
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
        DockerOps::check_daemon_running().map_err(|_| errors::provider::docker_connection_failed())
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
        self.check_daemon_is_running()?;
        self.handle_potential_issues();
        self.check_docker_build_requirements();

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
        let compose_path = compose_ops.write_docker_compose(&build_context)?;

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

        stream_command("docker", &all_args).with_context(|| {
            format!("Docker build failed for project '{}'. Check that Docker is running and build context is valid", self.project_name())
        })?;

        // Step 5: Start containers
        let args = ComposeCommand::build_args(&compose_path, "up", &["-d"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command("docker", &args_refs).with_context(|| {
            format!(
                "Failed to start container '{}'. Container may have failed to start properly",
                self.container_name()
            )
        })?;

        self.provision_container()?;

        Ok(())
    }

    /// Create a container with a specific instance name
    #[must_use = "container creation results should be handled"]
    pub fn create_container_with_instance(&self, instance_name: &str) -> Result<()> {
        self.check_daemon_is_running()?;
        self.handle_potential_issues();
        self.check_docker_build_requirements();

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
        let compose_path =
            compose_ops.write_docker_compose_with_instance(&build_context, instance_name)?;

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

        stream_command("docker", &all_args).with_context(|| {
            format!("Docker build failed for project '{}' instance '{}'. Check that Docker is running and build context is valid", self.project_name(), instance_name)
        })?;

        // Step 5: Start containers
        let args = ComposeCommand::build_args(&compose_path, "up", &["-d"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command("docker", &args_refs).with_context(|| {
            format!(
                "Failed to start container '{}'. Container may have failed to start properly",
                container_name
            )
        })?;

        self.provision_container_with_instance(instance_name)?;

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
            println!("\nWhat would you like to do?");
            if is_running {
                println!("  1. Keep using the existing running container");
            } else {
                println!("  1. Start the existing container");
            }
            println!("  2. Recreate the container (destroy and rebuild)");
            println!("  3. Cancel operation");

            print!("\nChoice [1-3]: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim() {
                "1" => {
                    if is_running {
                        vm_success!("Using existing running container.");
                        Ok(())
                    } else {
                        println!("\n▶️  Starting existing container...");
                        self.start_container(None)
                    }
                }
                "2" => {
                    println!("\n🔄 Recreating container...");
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
            Err(anyhow::anyhow!(
                "Container '{}' already exists. In non-interactive mode, please use:\n\
                 - 'vm start' to start the existing container\n\
                 - 'vm destroy' followed by 'vm create' to recreate it",
                container_name
            ))
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
            println!("\nWhat would you like to do?");
            if is_running {
                println!("  1. Keep using the existing running container");
            } else {
                println!("  1. Start the existing container");
            }
            println!("  2. Recreate the container (destroy and rebuild)");
            println!("  3. Cancel operation");

            print!("\nChoice [1-3]: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim() {
                "1" => {
                    if is_running {
                        vm_success!("Using existing running container.");
                        Ok(())
                    } else {
                        println!("\n▶️  Starting existing container...");
                        self.start_container(Some(&container_name))
                    }
                }
                "2" => {
                    println!("\n🔄 Recreating container...");
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
            Err(anyhow::anyhow!(
                "Container '{}' already exists. In non-interactive mode, please use:\n\
                 - 'vm start {}' to start the existing container\n\
                 - 'vm destroy {}' followed by 'vm create --instance {}' to recreate it",
                container_name,
                container_name,
                container_name,
                instance_name
            ))
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
                serde_json::to_value(container_pipx_packages).with_context(|| {
                    "Failed to serialize pipx package list for container configuration"
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
        fs::write(&temp_config_path, config_json).with_context(|| {
            format!(
                "Failed to write configuration to {}",
                temp_config_path.display()
            )
        })?;
        let source = BuildOperations::path_to_string(&temp_config_path)?;
        let destination = format!("{}:{}", self.container_name(), TEMP_CONFIG_PATH);
        let copy_result = DockerOps::copy(source, &destination);
        if copy_result.is_err() {
            return Err(anyhow::anyhow!(
                "Failed to copy VM configuration to container '{}'. Container may not be running or accessible",
                self.container_name()
            ));
        }
        Ok(())
    }

    #[must_use = "container provisioning results should be handled"]
    fn provision_container(&self) -> Result<()> {
        // Step 6: Wait for readiness and run ansible (configuration only)
        let mut attempt = 1;
        while attempt <= CONTAINER_READINESS_MAX_ATTEMPTS {
            if DockerOps::test_container_readiness(&self.container_name()) {
                break;
            }
            if attempt == CONTAINER_READINESS_MAX_ATTEMPTS {
                vm_error!("Container failed to become ready");
                return Err(anyhow::anyhow!(
                    "Container '{}' failed to become ready after {} seconds. Container may be unhealthy or not starting properly",
                    self.container_name(),
                    u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS
                ));
            }
            std::thread::sleep(std::time::Duration::from_secs(
                CONTAINER_READINESS_SLEEP_SECONDS,
            ));
            attempt += 1;
        }

        self.prepare_and_copy_config()?;

        stream_command(
            "docker",
            &[
                "exec",
                &self.container_name(),
                "bash",
                "-c",
                &format!(
                    "ansible-playbook -i localhost, -c local {} -vvv",
                    ANSIBLE_PLAYBOOK_PATH
                ),
            ],
        )
        .with_context(|| {
            format!("Ansible provisioning failed for container '{}'. Check container logs for detailed error information", self.container_name())
        })?;

        // Start services silently
        DockerOps::exec_in_container(
            &self.container_name(),
            &["bash", "-c", "supervisorctl reread && supervisorctl update"],
        )
        .ok();

        Ok(())
    }

    /// Provision container with custom instance name
    fn provision_container_with_instance(&self, instance_name: &str) -> Result<()> {
        let container_name = self.container_name_with_instance(instance_name);

        // Step 6: Wait for readiness and run ansible (configuration only)
        let mut attempt = 1;
        while attempt <= CONTAINER_READINESS_MAX_ATTEMPTS {
            if DockerOps::test_container_readiness(&container_name) {
                break;
            }
            if attempt == CONTAINER_READINESS_MAX_ATTEMPTS {
                vm_error!("Container failed to become ready");
                return Err(anyhow::anyhow!(
                    "Container '{}' failed to become ready after {} seconds. Container may be unhealthy or not starting properly",
                    container_name,
                    u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS
                ));
            }
            std::thread::sleep(std::time::Duration::from_secs(
                CONTAINER_READINESS_SLEEP_SECONDS,
            ));
            attempt += 1;
        }

        self.prepare_and_copy_config()?;

        stream_command(
            "docker",
            &[
                "exec",
                &container_name,
                "bash",
                "-c",
                &format!(
                    "ansible-playbook -i localhost, -c local {} -vvv",
                    ANSIBLE_PLAYBOOK_PATH
                ),
            ],
        )
        .with_context(|| {
            format!("Ansible provisioning failed for container '{}'. Check container logs for detailed error information", container_name)
        })?;

        // Start services silently
        DockerOps::exec_in_container(
            &container_name,
            &["bash", "-c", "supervisorctl reread && supervisorctl update"],
        )
        .ok();

        Ok(())
    }

    #[must_use = "container start results should be handled"]
    pub fn start_container(&self, container: Option<&str>) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;
        stream_command("docker", &["start", &target_container])
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
            .with_context(|| format!("Failed to stop container '{}'", target_container))?;
        Ok(())
    }

    #[must_use = "container destruction results should be handled"]
    pub fn destroy_container(&self, container: Option<&str>) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;
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
            println!();
            println!("  User:  {}", project_user);
            println!("  Path:  {}", target_dir);
            println!("  Shell: {}", shell);
            println!("\n💡 Exit with: exit or Ctrl-D\n");
        }

        // First check if container exists to provide better error messages
        let container_exists = duct::cmd("docker", &["inspect", &container_name])
            .stdout_null()
            .stderr_null()
            .run()
            .is_ok();

        if !container_exists {
            // Return error without printing raw Docker messages
            return Err(anyhow::anyhow!("No such container: {}", container_name));
        }

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
                        Err(
                            ProviderError::CommandFailed(String::from("SSH connection lost"))
                                .into(),
                        )
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
                        Err(anyhow::anyhow!("Docker command failed"))
                    } else {
                        Err(anyhow::anyhow!("Command execution failed"))
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
    }

    #[must_use = "status display results should be handled"]
    pub fn show_status(&self, container: Option<&str>) -> Result<()> {
        let compose_ops = ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
        if compose_ops.status_with_compose().is_err() {
            stream_command(
                "docker",
                &[
                    "ps",
                    "-f",
                    &format!("name={}", self.resolve_target_container(container)?),
                ],
            )?;
        }

        if let Some(range) = &self.config.ports.range {
            if range.len() == 2 {
                let (start, end) = (range[0], range[1]);
                println!("\n📡 Port Range: {}-{}", start, end);
                if !self.config.ports.manual_ports.is_empty() {
                    let used_count = self
                        .config
                        .ports
                        .manual_ports
                        .values()
                        .filter(|&&p| p >= start && p <= end)
                        .count();
                    println!("   Used: {}/{} ports", used_count, (end - start + 1));
                }
            }
        }
        Ok(())
    }

    #[must_use = "container restart results should be handled"]
    pub fn restart_container(&self, container: Option<&str>) -> Result<()> {
        self.stop_container(container)?;
        self.start_container(container)
    }

    #[must_use = "existing container provisioning results should be handled"]
    pub fn provision_existing(&self, container: Option<&str>) -> Result<()> {
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
            return Err(anyhow::anyhow!(
                "Container {} is not running. Start it first with 'vm start'",
                target_container
            ));
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
                .with_context(|| {
                    format!("Failed to copy Ansible provisioning resources to container '{}'. Container may not be accessible", &target_container)
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
                    "ansible-playbook -i localhost, -c local {} -vvv",
                    ANSIBLE_PLAYBOOK_PATH
                ),
            ],
        );
        if let Err(e) = ansible_result {
            ProgressReporter::error("└─", &format!("Provisioning failed: {}", e));
            return Err(e).with_context(|| {
                format!("Ansible provisioning failed during setup for container '{}'. Check provisioning logs for details", target_container)
            });
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
            .with_context(|| "Failed to get container information from Docker. Ensure Docker is running and accessible")?;

        if !ps_output.status.success() {
            return Err(anyhow::anyhow!(
                "Docker ps command failed. Check that Docker is installed, running, and accessible"
            ));
        }

        // Get stats for running containers
        let stats_output = DockerCommand::new()
            .subcommand("stats")
            .arg("--no-stream")
            .arg("--format")
            .arg("{{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}")
            .execute_raw()
            .with_context(|| "Failed to get container resource statistics from Docker. Some containers may not be running")?;

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
        stream_command("docker", &["kill", &target_container]).with_context(|| {
            format!("Failed to kill container '{}'. Container may not exist or Docker may be unresponsive", &target_container)
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
            .with_context(|| "Failed to list containers for name resolution. Docker may not be running or accessible")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Docker container listing failed during name resolution. Check Docker daemon status"
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
            0 => Err(anyhow::anyhow!(
                "No container found matching '{}'. Use 'vm list' to see available containers",
                partial_name
            )),
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
            return Err(anyhow::anyhow!(
                "Container '{}' is not healthy after mount update. Check container logs for issues",
                &state.container_name
            ));
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
}
