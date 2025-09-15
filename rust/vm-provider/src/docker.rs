// Standard library
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

// External crates
use anyhow::{Context, Result};
use is_terminal::IsTerminal;
use tera::{Context as TeraContext, Tera};

// External dependencies

// Internal imports
use crate::{
    audio::MacOSAudioManager,
    error::ProviderError,
    preflight,
    progress::ProgressReporter,
    resources,
    security::SecurityValidator,
    utils::{is_tool_installed, stream_command, stream_docker_build},
    Provider, TempProvider, TempVmState,
};
use vm_config::config::VmConfig;

pub struct DockerProvider {
    config: VmConfig,
    _project_dir: PathBuf, // The root of the user's project
    temp_dir: PathBuf, // Persistent project-specific directory for generated files like docker-compose.yml
}

impl DockerProvider {
    /// Safely convert a path to string with descriptive error message
    fn path_to_string(path: &Path) -> Result<&str> {
        path.to_str().ok_or_else(|| {
            anyhow::anyhow!("Path contains invalid UTF-8 characters: {}", path.display())
        })
    }

    /// Prepare build context with embedded resources and generated Dockerfile
    fn prepare_build_context(&self) -> Result<PathBuf> {
        // Create temporary build context directory
        let build_context = self.temp_dir.join("build_context");
        if build_context.exists() {
            fs::remove_dir_all(&build_context)?;
        }
        fs::create_dir_all(&build_context)?;

        // Create shared directory and copy embedded resources
        let shared_dir = build_context.join("shared");
        fs::create_dir_all(&shared_dir)?;

        // Copy embedded resources to build context
        resources::copy_embedded_resources(&shared_dir)?;

        // Generate Dockerfile from template
        let dockerfile_path = build_context.join("Dockerfile.generated");
        self.generate_dockerfile(&dockerfile_path)?;

        Ok(build_context)
    }

    /// Generate Dockerfile from template with build args
    fn generate_dockerfile(&self, output_path: &Path) -> Result<()> {
        let mut tera = Tera::default();
        let template_content = include_str!("docker/Dockerfile.j2");
        tera.add_raw_template("Dockerfile", template_content)?;

        let current_uid = vm_config::get_current_uid();
        let current_gid = vm_config::get_current_gid();
        let project_user = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

        let mut context = TeraContext::new();
        context.insert("project_uid", &current_uid.to_string());
        context.insert("project_gid", &current_gid.to_string());
        context.insert("project_user", &project_user);

        let content = tera.render("Dockerfile", &context)?;
        fs::write(output_path, content.as_bytes())?;

        Ok(())
    }

    /// Gather all package lists and format as build arguments
    fn gather_build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // Add version build args
        if let Some(versions) = &self.config.versions {
            if let Some(node) = &versions.node {
                args.push(format!("--build-arg=NODE_VERSION={}", node));
            }
            if let Some(nvm) = &versions.nvm {
                args.push(format!("--build-arg=NVM_VERSION={}", nvm));
            }
            if let Some(pnpm) = &versions.pnpm {
                args.push(format!("--build-arg=PNPM_VERSION={}", pnpm));
            }
        }

        // Add package list build args
        if !self.config.apt_packages.is_empty() {
            let packages = self.config.apt_packages.join(" ");
            args.push(format!("--build-arg=APT_PACKAGES={}", packages));
        }

        if !self.config.npm_packages.is_empty() {
            let packages = self.config.npm_packages.join(" ");
            args.push(format!("--build-arg=NPM_PACKAGES={}", packages));
        }

        if !self.config.pip_packages.is_empty() {
            let packages = self.config.pip_packages.join(" ");
            args.push(format!("--build-arg=PIP_PACKAGES={}", packages));
        }

        if !self.config.pipx_packages.is_empty() {
            let packages = self.config.pipx_packages.join(" ");
            args.push(format!("--build-arg=PIPX_PACKAGES={}", packages));
        }

        if !self.config.cargo_packages.is_empty() {
            let packages = self.config.cargo_packages.join(" ");
            args.push(format!("--build-arg=CARGO_PACKAGES={}", packages));
        }

        // Add user/group build args
        let current_uid = vm_config::get_current_uid();
        let current_gid = vm_config::get_current_gid();
        let project_user = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

        args.push(format!("--build-arg=PROJECT_UID={}", current_uid));
        args.push(format!("--build-arg=PROJECT_GID={}", current_gid));
        args.push(format!("--build-arg=PROJECT_USER={}", project_user));

        args
    }

    /// Handle potential Docker issues proactively
    fn handle_potential_issues(&self) -> Result<()> {
        // Check for port conflicts and provide helpful guidance
        if let Some(vm_config) = &self.config.vm {
            if vm_config.memory.unwrap_or(0) > 8192 {
                eprintln!("💡 Tip: High memory allocation detected. Ensure your system has sufficient RAM.");
            }
        }

        // Check Docker daemon status more thoroughly
        if std::process::Command::new("docker")
            .arg("ps")
            .output()
            .is_err()
        {
            eprintln!("⚠️  Docker daemon may not be responding properly");
            eprintln!("   Try: docker system prune -f");
        }

        Ok(())
    }

    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("docker") {
            return Err(ProviderError::DependencyNotFound(String::from("Docker")).into());
        }

        let project_dir = std::env::current_dir()?;

        // Create a persistent project-specific directory for docker-compose files
        let project_name = config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project");

        let temp_dir = std::env::temp_dir().join(format!("vm-{}", project_name));
        fs::create_dir_all(&temp_dir).context("Failed to create project-specific directory")?;

        Ok(Self {
            config,
            _project_dir: project_dir,
            temp_dir,
        })
    }

    fn check_daemon_is_running(&self) -> Result<()> {
        let status = std::process::Command::new("docker").arg("info").output();
        if let Ok(output) = status {
            if output.status.success() {
                return Ok(());
            }
        }
        Err(ProviderError::DependencyNotFound(
            "Docker daemon is not running. Please start Docker Desktop or the Docker service."
                .to_string(),
        )
        .into())
    }

    fn project_name(&self) -> &str {
        self.config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project")
    }

    fn container_name(&self) -> String {
        format!("{}-dev", self.project_name())
    }

    fn load_docker_compose_template() -> Result<String> {
        // Try to load from file first (for development/customization)
        let template_path =
            vm_config::get_tool_dir().join("rust/vm-provider/src/docker/template.yml");

        if template_path.exists() {
            eprintln!(
                "Loading Docker Compose template from: {}",
                template_path.display()
            );
            std::fs::read_to_string(&template_path)
                .with_context(|| format!("Failed to read template from {:?}", template_path))
        } else {
            // Fallback to embedded template for production
            eprintln!("Using embedded Docker Compose template");
            Ok(String::from(include_str!("docker/template.yml")))
        }
    }

    fn render_docker_compose(&self, build_context_dir: &Path) -> Result<String> {
        let mut tera = Tera::default();
        let template_content = Self::load_docker_compose_template()?;
        tera.add_raw_template("docker-compose.yml", &template_content)?;

        let project_dir_str = Self::path_to_string(&self._project_dir)?;
        let build_context_str = Self::path_to_string(build_context_dir)?;

        let current_uid = vm_config::get_current_uid();
        let current_gid = vm_config::get_current_gid();
        let project_user = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

        let mut context = TeraContext::new();
        context.insert("config", &self.config);
        context.insert("project_dir", &project_dir_str);
        context.insert("build_context_dir", &build_context_str);
        context.insert("project_uid", &current_uid.to_string());
        context.insert("project_gid", &current_gid.to_string());
        context.insert("project_user", &project_user);
        context.insert("is_macos", &cfg!(target_os = "macos"));

        let content = tera.render("docker-compose.yml", &context)?;
        Ok(content)
    }

    fn write_docker_compose(&self, build_context_dir: &Path) -> Result<PathBuf> {
        let content = self.render_docker_compose(build_context_dir)?;

        let path = self.temp_dir.join("docker-compose.yml");
        fs::write(&path, content.as_bytes())?;

        Ok(path)
    }
}

impl Provider for DockerProvider {
    fn name(&self) -> &'static str {
        "docker"
    }

    fn create(&self) -> Result<()> {
        preflight::check_system_resources()?;
        self.check_daemon_is_running()?;
        self.handle_potential_issues()?;

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
            .unwrap_or(false);

        if container_exists {
            // Check if it's running
            let is_running = std::process::Command::new("docker")
                .args(["ps", "--format", "{{.Names}}"])
                .output()
                .map(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .any(|line| line.trim() == container_name)
                })
                .unwrap_or(false);

            let status = if is_running { "running" } else { "stopped" };
            println!("⚠️  Container '{}' already exists (status: {}).", container_name, status);

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
                            println!("\n✅ Using existing running container.");
                            return Ok(());
                        } else {
                            println!("\n▶️  Starting existing container...");
                            return self.start();
                        }
                    }
                    "2" => {
                        println!("\n🔄 Recreating container...");
                        self.destroy()?;
                        // Continue with creation below
                    }
                    _ => {
                        println!("❌ Operation cancelled.");
                        return Ok(());
                    }
                }
            } else {
                // Non-interactive mode: fail with informative error
                return Err(anyhow::anyhow!(
                    "Container '{}' already exists. In non-interactive mode, please use:\n\
                     - 'vm start' to start the existing container\n\
                     - 'vm destroy' followed by 'vm create' to recreate it",
                    container_name
                ));
            }
        }

        // Only setup audio if it's enabled in the configuration
        if let Some(audio_service) = self.config.services.get("audio") {
            if audio_service.enabled {
                MacOSAudioManager::setup()?;
            }
        }

        println!("🐳 Creating Docker Environment");

        // Step 1: Prepare build context with embedded resources
        print!("📦 Preparing build context... ");
        let build_context = self.prepare_build_context()?;
        println!("✅");

        // Step 2: Generate docker-compose.yml with build context
        print!("📄 Generating docker-compose.yml... ");
        let compose_path = self.write_docker_compose(&build_context)?;
        println!("✅");

        // Step 3: Gather build arguments for packages
        let build_args = self.gather_build_args();

        // Step 4: Build with all package arguments
        println!("\n🔨 Building container image with packages (this may take a while)...");
        let mut docker_build_args = vec![
            String::from("compose"),
            String::from("-f"),
            Self::path_to_string(&compose_path)?.to_string(),
            String::from("build"),
        ];
        docker_build_args.extend(build_args);

        let docker_build_str_args: Vec<&str> =
            docker_build_args.iter().map(|s| s.as_str()).collect();
        stream_docker_build(&docker_build_str_args).context("Docker build failed")?;
        println!("✅ Build complete");

        // Step 5: Start containers
        println!("\n🚀 Starting containers");
        stream_command(
            "docker",
            &[
                "compose",
                "-f",
                Self::path_to_string(&compose_path)?,
                "up",
                "-d",
            ],
        )
        .context("Docker up failed")?;
        println!("✅ Containers started");

        // Step 6: Wait for readiness and run ansible (configuration only)
        println!("\n🔧 Provisioning environment");
        print!("⏳ Waiting for container readiness... ");
        let max_attempts = 30;
        let mut attempt = 1;
        while attempt <= max_attempts {
            if let Ok(output) = std::process::Command::new("docker")
                .args(["exec", &self.container_name(), "echo", "ready"])
                .output()
            {
                if output.status.success() {
                    break;
                }
            }
            if attempt == max_attempts {
                println!("❌");
                return Err(anyhow::anyhow!("Container failed to become ready"));
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
            attempt += 1;
        }
        println!("✅");

        print!("📋 Loading project configuration... ");
        let config_json = self.config.to_json()?;
        let temp_config_path = self.temp_dir.join("vm-config.json");
        fs::write(&temp_config_path, config_json)?;
        let copy_result = std::process::Command::new("docker")
            .args([
                "cp",
                Self::path_to_string(&temp_config_path)?,
                &format!("{}:/tmp/vm-config.json", self.container_name()),
            ])
            .output()?;
        if !copy_result.status.success() {
            println!("❌");
            return Err(anyhow::anyhow!("Failed to copy config to container"));
        }
        println!("✅");

        println!("🎭 Running Ansible provisioning (configuration only)...");
        stream_command(
            "docker",
            &[
                "exec",
                &self.container_name(),
                "bash",
                "-c",
                "ansible-playbook -i localhost, -c local /app/shared/ansible/playbook.yml",
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

        println!("\n✅ Docker environment created successfully!");
        println!("🎉 All packages installed at build time for faster startup!");
        Ok(())
    }

    fn start(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            // Fallback: prepare build context and generate compose file
            let build_context = self.prepare_build_context()?;
            self.write_docker_compose(&build_context)?;
        }
        stream_command(
            "docker",
            &[
                "compose",
                "-f",
                Self::path_to_string(&compose_path)?,
                "up",
                "-d",
            ],
        )
        .context("Failed to start container with docker-compose")
    }

    fn stop(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if compose_path.exists() {
            stream_command(
                "docker",
                &[
                    "compose",
                    "-f",
                    Self::path_to_string(&compose_path)?,
                    "stop",
                ],
            )
            .context("Failed to stop container with docker-compose")
        } else {
            stream_command("docker", &["stop", &self.container_name()])
                .context("Failed to stop container")
        }
    }

    fn destroy(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            println!("docker-compose.yml not found. Attempting to remove container by name.");
            return stream_command("docker", &["rm", "-f", &self.container_name()]);
        }
        stream_command(
            "docker",
            &[
                "compose",
                "-f",
                Self::path_to_string(&compose_path)?,
                "down",
                "--volumes",
            ],
        )?;

        // Only cleanup audio if it was enabled in the configuration
        if let Some(audio_service) = self.config.services.get("audio") {
            if audio_service.enabled {
                MacOSAudioManager::cleanup()?;
            }
        }

        Ok(())
    }

    fn ssh(&self, relative_path: &Path) -> Result<()> {
        let workspace_path = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace");
        let project_user = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");
        let shell = self
            .config
            .terminal
            .as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or("zsh");

        let target_path = SecurityValidator::validate_relative_path(relative_path, workspace_path)?;
        let target_dir = target_path.to_string_lossy().to_string();

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

    fn exec(&self, cmd: &[String]) -> Result<()> {
        let container = self.container_name();
        let workspace_path = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace");
        let project_user = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

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

    fn logs(&self) -> Result<()> {
        stream_command("docker", &["logs", "-f", &self.container_name()])
    }

    fn status(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if compose_path.exists() {
            stream_command(
                "docker",
                &["compose", "-f", Self::path_to_string(&compose_path)?, "ps"],
            )?;
        } else {
            stream_command(
                "docker",
                &["ps", "-f", &format!("name={}", self.container_name())],
            )?;
        }

        if let Some(port_range) = &self.config.port_range {
            println!("\n📡 Port Range: {}", port_range);
            if !self.config.ports.is_empty() {
                if let Ok(parsed_range) = vm_ports::PortRange::parse(port_range) {
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

    fn restart(&self) -> Result<()> {
        self.stop()?;
        self.start()
    }

    fn provision(&self) -> Result<()> {
        let container = self.container_name();
        let status_output = std::process::Command::new("docker")
            .args(["inspect", "--format", "{{.State.Status}}", &container])
            .output()?;
        let status = String::from_utf8_lossy(&status_output.stdout)
            .trim()
            .to_string();
        if status != "running" {
            return Err(anyhow::anyhow!(
                "Container {} is not running. Start it first with 'vm start'",
                container
            ));
        }

        let progress = ProgressReporter::new();
        progress.phase_header("🔧", "PROVISIONING PHASE");
        progress.subtask("├─", "Re-running Ansible provisioning...");

        let config_json = self.config.to_json()?;
        let temp_config_path = self.temp_dir.join("vm-config.json");
        std::fs::write(&temp_config_path, config_json)?;
        stream_command(
            "docker",
            &[
                "cp",
                Self::path_to_string(&temp_config_path)?,
                &format!("{}:/tmp/vm-config.json", self.container_name()),
            ],
        )?;

        let ansible_result = stream_command(
            "docker",
            &[
                "exec",
                &self.container_name(),
                "bash",
                "-c",
                "ansible-playbook -i localhost, -c local /app/shared/ansible/playbook.yml",
            ],
        );
        if ansible_result.is_err() {
            progress.error("└─", "Provisioning failed");
            return Err(anyhow::anyhow!("Ansible provisioning failed"));
        }
        progress.complete("└─", "Provisioning complete");
        Ok(())
    }

    fn list(&self) -> Result<()> {
        // Show all containers - let users see the full Docker environment
        // This provides better visibility than filtering by project name
        stream_command("docker", &["ps", "-a"]).context("Failed to list containers")
    }

    fn kill(&self, container: Option<&str>) -> Result<()> {
        let container_name = self.container_name();
        let target_container = container.unwrap_or(&container_name);
        stream_command("docker", &["kill", target_container]).context("Failed to kill container")
    }

    fn get_sync_directory(&self) -> Result<String> {
        Ok(self
            .config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace")
            .to_string())
    }

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
    }
}

impl TempProvider for DockerProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        let progress = ProgressReporter::new();
        let main_phase = progress.start_phase("Updating container mounts");
        progress.task(&main_phase, "Checking container status...");

        if self.is_container_running(&state.container_name)? {
            progress.task(&main_phase, "Stopping container...");
            stream_command("docker", &["stop", &state.container_name])?;
        }

        progress.task(&main_phase, "Recreating container with new mounts...");
        self.recreate_with_mounts(state)?;

        progress.task(&main_phase, "Starting container...");
        let compose_path = self.temp_dir.join("docker-compose.yml");
        stream_command(
            "docker",
            &[
                "compose",
                "-f",
                Self::path_to_string(&compose_path)?,
                "up",
                "-d",
            ],
        )?;

        progress.task(&main_phase, "Checking container health...");
        if !self.check_container_health(&state.container_name)? {
            progress.finish_phase(&main_phase, "Mount update failed - container not healthy");
            return Err(anyhow::anyhow!(
                "Container is not healthy after mount update"
            ));
        }

        progress.finish_phase(&main_phase, "Mounts updated successfully");
        Ok(())
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        let progress = ProgressReporter::new();
        let phase = progress.start_phase("Recreating container configuration");
        progress.task(&phase, "Generating updated docker-compose.yml...");

        let mut temp_config = self.config.clone();
        if let Some(ref mut project) = temp_config.project {
            project.name = Some(String::from("vm-temp"));
        }

        let content = self.render_docker_compose_with_mounts(state)?;
        let compose_path = self.temp_dir.join("docker-compose.yml");
        fs::write(&compose_path, content.as_bytes())?;

        progress.task(&phase, "Removing old container...");
        let _ = stream_command("docker", &["rm", "-f", &state.container_name]);

        progress.finish_phase(&phase, "Container configuration updated");
        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        let max_attempts = 30;
        for _ in 0..max_attempts {
            if stream_command("docker", &["exec", container_name, "echo", "ready"]).is_ok() {
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
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
        let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(status == "running")
    }
}

impl DockerProvider {
    fn render_docker_compose_with_mounts(&self, state: &TempVmState) -> Result<String> {
        const TEMP_DOCKER_COMPOSE_TEMPLATE: &str = r#"
# Temporary VM docker-compose.yml with custom mounts
services:
  {{ container_name }}:
    container_name: {{ container_name }}
    build:
      context: ..
      dockerfile: Dockerfile
    image: vm-temp:latest
    volumes:
      # Default workspace volume
      - ../..:/workspace:rw
      # Persistent volumes for cache and package managers
      - vmtemp_nvm:/home/developer/.nvm
      - vmtemp_cache:/home/developer/.cache
      {% if mounts %}# Custom temp VM mounts
      {% for mount in mounts %}- {{ mount.source }}:{{ mount.target }}:{{ mount.permissions }}
      {% endfor %}{% endif %}
    ports:
      {% if config.ports %}{% for name, port in config.ports %}- "{{ port }}:{{ port }}"
      {% endfor %}{% endif %}
    environment:
      {% if config.environment %}{% for name, value in config.environment %}- {{ name }}={{ value }}
      {% endfor %}{% endif %}
    cap_add:
      - SYS_PTRACE
    security_opt:
      - seccomp=unconfined

volumes:
  vmtemp_nvm:
  vmtemp_cache:
"#;
        let mut tera = Tera::default();
        tera.add_raw_template("docker-compose.yml", TEMP_DOCKER_COMPOSE_TEMPLATE)?;

        let mut context = TeraContext::new();
        context.insert("config", &self.config);
        context.insert("container_name", &state.container_name);
        context.insert("mounts", &state.mounts);

        let content = tera.render("docker-compose.yml", &context)?;
        Ok(content)
    }
}
