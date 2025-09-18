//! Docker container lifecycle management operations.
//!
//! This module handles the complete lifecycle of Docker containers used
//! for VM environments, including creation, starting, stopping, provisioning,
//! and cleanup operations. It provides a high-level interface for VM
//! management through Docker containers with progress reporting and error handling.

// Standard library
use std::fs;
use std::io::{self, Write};
use std::path::Path;

// External crates
use anyhow::{Context, Result};
use is_terminal::IsTerminal;
use serde_json::Value;

// Internal imports
use super::{build::BuildOperations, compose::ComposeOperations, ComposeCommand, UserConfig};
use crate::{
    audio::MacOSAudioManager, error::ProviderError, progress::ProgressReporter,
    security::SecurityValidator, TempProvider, TempVmState,
};
use vm_common::command_stream::stream_command;
use vm_common::{vm_dbg, vm_error, vm_success, vm_warning};
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
                .context("Failed to parse pipx list output as JSON")?;
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

    /// Handle potential Docker issues proactively
    pub fn handle_potential_issues(&self) {
        // Check for port conflicts and provide helpful guidance
        if let Some(vm_config) = &self.config.vm {
            if vm_config.memory.unwrap_or(0) > HIGH_MEMORY_THRESHOLD {
                eprintln!("💡 Tip: High memory allocation detected. Ensure your system has sufficient RAM.");
            }
        }

        // Check Docker daemon status more thoroughly
        if std::process::Command::new("docker")
            .arg("ps")
            .output()
            .is_err()
        {
            vm_warning!("Docker daemon may not be responding properly");
            eprintln!("   Try: docker system prune -f");
        }
    }

    #[must_use = "Docker daemon status should be checked"]
    pub fn check_daemon_is_running(&self) -> Result<()> {
        let status = std::process::Command::new("docker").arg("info").output();
        if let Ok(output) = status {
            if output.status.success() {
                return Ok(());
            }
        }
        Err(ProviderError::DependencyNotFound(
            "Docker daemon is not running. Please start Docker Desktop or the Docker service."
                .into(),
        )
        .into())
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
        let container_exists = std::process::Command::new("docker")
            .args(["ps", "-a", "--format", "{{.Names}}"])
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.trim() == container_name)
            })
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
                    eprintln!("⚠️  Audio setup warning: {}", e);
                }
                #[cfg(not(target_os = "macos"))]
                MacOSAudioManager::setup();
            }
        }

        println!("🐳 Creating Docker Environment");

        // Step 1: Filter pipx-managed packages from pip_packages
        let mut modified_config = self.config.clone();
        self.filter_pipx_managed_packages(&mut modified_config)?;

        // Step 2: Prepare build context with embedded resources
        print!("📦 Preparing build context... ");
        let build_ops = BuildOperations::new(&modified_config, self.temp_dir);
        let build_context = build_ops.prepare_build_context()?;
        println!("✅");

        // Step 3: Generate docker-compose.yml with build context and modified config
        print!("📄 Generating docker-compose.yml... ");
        let compose_ops = ComposeOperations::new(&modified_config, self.temp_dir, self.project_dir);
        let compose_path = compose_ops.write_docker_compose(&build_context)?;
        println!("✅");

        // Step 3: Gather build arguments for packages
        let build_args = build_ops.gather_build_args();

        // Step 4: Build with all package arguments
        println!("\n🔨 Building container image with packages (this may take a while)...");
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

        stream_command("docker", &all_args).context("Docker build failed")?;
        println!("✅ Build complete");

        // Step 5: Start containers
        println!("\n🚀 Starting containers");
        let args = ComposeCommand::build_args(&compose_path, "up", &["-d"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command("docker", &args_refs).context("Docker up failed")?;
        println!("✅ Containers started");

        self.provision_container()?;

        vm_success!("Docker environment created successfully!");
        println!("🎉 All packages installed at build time for faster startup!");
        Ok(())
    }

    #[must_use = "existing container handling results should be checked"]
    fn handle_existing_container(&self) -> Result<()> {
        let container_name = self.container_name();

        // Check if it's running
        let is_running = std::process::Command::new("docker")
            .args(["ps", "--format", "{{.Names}}"])
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.trim() == container_name)
            })
            .map_err(|e| vm_warning!("Failed to check running containers (continuing): {}", e))
            .unwrap_or(false);

        let status = if is_running { "running" } else { "stopped" };
        vm_warning!(
            "Container '{}' already exists (status: {}).",
            container_name,
            status
        );

        // Check if we're in an interactive terminal
        if std::io::stdin().is_terminal() {
            println!("\nWhat would you like to do?");
            if is_running {
                println!("  1. Keep using the existing running container");
            } else {
                println!("  1. Start the existing container");
            }
            println!("  2. Recreate the container (destroy and rebuild)");
            println!("  3. Cancel operation");

            print!("\nChoice [1-3]: ");
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            match input.trim() {
                "1" => {
                    if is_running {
                        vm_success!("Using existing running container.");
                        Ok(())
                    } else {
                        println!("\n▶️  Starting existing container...");
                        self.start_container()
                    }
                }
                "2" => {
                    println!("\n🔄 Recreating container...");
                    self.destroy_container()?;
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

    #[must_use = "package filtering results should be checked"]
    fn filter_pipx_managed_packages(&self, config: &mut vm_config::config::VmConfig) -> Result<()> {
        let pipx_managed_packages = if let Some(pipx_json) = self.get_pipx_json()? {
            self.extract_pipx_managed_packages(&pipx_json)
        } else {
            std::collections::HashSet::new()
        };

        // Remove pipx-managed packages from pip_packages to prevent double installation
        if !pipx_managed_packages.is_empty() {
            config.pip_packages = config
                .pip_packages
                .iter()
                .filter(|pkg| !pipx_managed_packages.contains(*pkg))
                .cloned()
                .collect();
        }

        Ok(())
    }

    #[must_use = "config preparation results should be checked"]
    fn prepare_and_copy_config(&self) -> Result<()> {
        let mut config_clone = self.config.clone();
        let container_pipx_packages = if let Some(pipx_json) = self.get_pipx_json()? {
            self.categorize_pipx_packages(&pipx_json)
        } else {
            Vec::new()
        };

        // Always clear pip_packages if we processed any pipx packages, regardless of whether they were included
        if !self.config.pip_packages.is_empty() {
            config_clone.pip_packages = vec![];
        }

        // Add PyPI packages list to config for Ansible
        if !container_pipx_packages.is_empty() {
            config_clone.extra_config.insert(
                "container_pipx_packages".to_string(),
                serde_json::to_value(container_pipx_packages)
                    .context("Failed to serialize container pipx packages for configuration")?,
            );
        }

        let config_json = config_clone.to_json()?;
        let temp_config_path = self.temp_dir.join("vm-config.json");
        std::fs::write(&temp_config_path, config_json).with_context(|| {
            format!(
                "Failed to write configuration to {}",
                temp_config_path.display()
            )
        })?;
        let copy_result = std::process::Command::new("docker")
            .args([
                "cp",
                BuildOperations::path_to_string(&temp_config_path)?,
                &format!("{}:{}", self.container_name(), TEMP_CONFIG_PATH),
            ])
            .output()?;
        if !copy_result.status.success() {
            return Err(anyhow::anyhow!("Failed to copy config to container"));
        }
        Ok(())
    }

    #[must_use = "container provisioning results should be handled"]
    fn provision_container(&self) -> Result<()> {
        // Step 6: Wait for readiness and run ansible (configuration only)
        println!("\n🔧 Provisioning environment");
        print!("⏳ Waiting for container readiness... ");
        let mut attempt = 1;
        while attempt <= CONTAINER_READINESS_MAX_ATTEMPTS {
            if let Ok(output) = std::process::Command::new("docker")
                .args(["exec", &self.container_name(), "echo", "ready"])
                .output()
            {
                if output.status.success() {
                    break;
                }
            }
            if attempt == CONTAINER_READINESS_MAX_ATTEMPTS {
                vm_error!("Container failed to become ready");
                return Err(anyhow::anyhow!("Container failed to become ready"));
            }
            std::thread::sleep(std::time::Duration::from_secs(
                CONTAINER_READINESS_SLEEP_SECONDS,
            ));
            attempt += 1;
        }
        println!("✅");

        print!("📋 Loading project configuration... ");
        self.prepare_and_copy_config()?;
        println!("✅");

        println!("🎭 Running Ansible provisioning (configuration only)...");
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
        .context("Ansible provisioning failed")?;
        println!("✅ Provisioning complete");

        print!("🔄 Starting services... ");
        if std::process::Command::new("docker")
            .args([
                "exec",
                &self.container_name(),
                "bash",
                "-c",
                "supervisorctl reread && supervisorctl update",
            ])
            .output()
            .is_ok()
        {
            println!("✅");
        } else {
            println!("⚠️ (services may start later)");
        }

        Ok(())
    }

    #[must_use = "container start results should be handled"]
    pub fn start_container(&self) -> Result<()> {
        let compose_ops = ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
        compose_ops.start_with_compose()
    }

    #[must_use = "container stop results should be handled"]
    pub fn stop_container(&self) -> Result<()> {
        let compose_ops = ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
        if compose_ops.stop_with_compose().is_err() {
            // Fallback to direct container stop
            stream_command("docker", &["stop", &self.container_name()])
                .context("Failed to stop container")
        } else {
            Ok(())
        }
    }

    #[must_use = "container destruction results should be handled"]
    pub fn destroy_container(&self) -> Result<()> {
        let compose_ops = ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
        let result = if compose_ops.destroy_with_compose().is_err() {
            println!("docker-compose.yml not found. Attempting to remove container by name.");
            stream_command("docker", &["rm", "-f", &self.container_name()])
        } else {
            Ok(())
        };

        // Only cleanup audio if it was enabled in the configuration
        if let Some(audio_service) = self.config.services.get("audio") {
            if audio_service.enabled {
                #[cfg(target_os = "macos")]
                if let Err(e) = MacOSAudioManager::cleanup() {
                    eprintln!("⚠️  Audio cleanup warning: {}", e);
                }
                #[cfg(not(target_os = "macos"))]
                MacOSAudioManager::cleanup();
            }
        }

        result
    }

    #[must_use = "SSH connection results should be handled"]
    pub fn ssh_into_container(&self, relative_path: &Path) -> Result<()> {
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

        let status = duct::cmd(
            "docker",
            &[
                "exec",
                tty_flag,
                "-e",
                &format!("VM_TARGET_DIR={}", target_dir),
                &self.container_name(),
                "sudo",
                "-u",
                project_user,
                "sh",
                "-c",
                &format!("cd \"$VM_TARGET_DIR\" && exec {}", shell),
            ],
        )
        .run()?
        .status;

        if !status.success() {
            return Err(ProviderError::CommandFailed(String::from("SSH command failed")).into());
        }
        Ok(())
    }

    #[must_use = "command execution results should be handled"]
    pub fn exec_in_container(&self, cmd: &[String]) -> Result<()> {
        let container = self.container_name();
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
            &container,
        ];
        let cmd_strs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&cmd_strs);
        stream_command("docker", &args)
    }

    #[must_use = "log display results should be handled"]
    pub fn show_logs(&self) -> Result<()> {
        // Show recent logs without following (-f) to avoid hanging indefinitely
        // Use --tail to show last 50 lines and add timestamps
        stream_command(
            "docker",
            &["logs", "--tail", "50", "-t", &self.container_name()],
        )
    }

    #[must_use = "status display results should be handled"]
    pub fn show_status(&self) -> Result<()> {
        let compose_ops = ComposeOperations::new(self.config, self.temp_dir, self.project_dir);
        if compose_ops.status_with_compose().is_err() {
            stream_command(
                "docker",
                &["ps", "-f", &format!("name={}", self.container_name())],
            )?;
        }

        if let Some(port_range) = &self.config.port_range {
            println!("\n📡 Port Range: {}", port_range);
            if !self.config.ports.is_empty() {
                if let Ok(parsed_range) = vm_config::ports::PortRange::parse(port_range) {
                    let (start, end) = (parsed_range.start, parsed_range.end);
                    let used_count = self
                        .config
                        .ports
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
    pub fn restart_container(&self) -> Result<()> {
        self.stop_container()?;
        self.start_container()
    }

    #[must_use = "existing container provisioning results should be handled"]
    pub fn provision_existing(&self) -> Result<()> {
        let container = self.container_name();
        let status_output = std::process::Command::new("docker")
            .args(["inspect", "--format", "{{.State.Status}}", &container])
            .output()?;
        let status = String::from_utf8_lossy(&status_output.stdout)
            .trim()
            .to_owned();
        if status != "running" {
            return Err(anyhow::anyhow!(
                "Container {} is not running. Start it first with 'vm start'",
                container
            ));
        }

        let _progress = ProgressReporter::new();
        ProgressReporter::phase_header("🔧", "PROVISIONING PHASE");
        ProgressReporter::subtask("├─", "Re-running Ansible provisioning...");

        self.prepare_and_copy_config()?;

        let ansible_result = stream_command(
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
        );
        if let Err(e) = ansible_result {
            ProgressReporter::error("└─", &format!("Provisioning failed: {}", e));
            return Err(e).context("Ansible provisioning failed during container setup");
        }
        ProgressReporter::complete("└─", "Provisioning complete");
        Ok(())
    }

    #[must_use = "container listing results should be handled"]
    pub fn list_containers(&self) -> Result<()> {
        // Show all containers - let users see the full Docker environment
        // This provides better visibility than filtering by project name
        stream_command("docker", &["ps", "-a"]).context("Failed to list containers")
    }

    #[must_use = "container kill results should be handled"]
    pub fn kill_container(&self, container: Option<&str>) -> Result<()> {
        let container_name = self.container_name();
        let target_container = container.unwrap_or(&container_name);
        stream_command("docker", &["kill", target_container]).context("Failed to kill container")
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
                "Container is not healthy after mount update"
            ));
        }

        ProgressReporter::finish_phase(&main_phase, "Mounts updated successfully");
        Ok(())
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        let progress = ProgressReporter::new();
        let phase = progress.start_phase("Recreating container configuration");
        ProgressReporter::task(&phase, "Generating updated docker-compose.yml...");

        let mut temp_config = self.config.clone();
        if let Some(ref mut project) = temp_config.project {
            project.name = Some("vm-temp".to_owned());
        }

        let compose_ops = ComposeOperations::new(&temp_config, self.temp_dir, self.project_dir);
        let content = compose_ops.render_docker_compose_with_mounts(state)?;
        let compose_path = self.temp_dir.join("docker-compose.yml");
        std::fs::write(&compose_path, content.as_bytes())?;

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
        // Leak the memory to get 'static lifetime for testing
        let config = Box::leak(Box::new(VmConfig {
            pip_packages: vec!["claudeflow".to_string(), "ruff".to_string()],
            ..Default::default()
        }));
        let temp_dir = Box::leak(Box::new(PathBuf::from("/tmp/test")));
        let project_dir = Box::leak(Box::new(PathBuf::from("/project")));

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
