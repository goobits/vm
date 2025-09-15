use crate::{
    audio::MacOSAudioManager,
    error::ProviderError,
    preflight,
    progress::ProgressReporter,
    resources,
    security::SecurityValidator,
    utils::{is_tool_installed, stream_command, stream_docker_build},
    Provider,
};
use crate::{ TempProvider, TempVmState };
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tera::{Context as TeraContext, Tera};
use vm_config::config::VmConfig;

// External dependencies
extern crate atty;

const DOCKERFILE_TEMPLATE: &str = include_str!("../../../providers/docker/Dockerfile");

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

    /// Legacy method - no longer needed with self-contained containers
    fn setup_container_resources(&self) -> Result<()> {
        // Resources are now embedded in the container image during build
        // This method is kept for compatibility but does nothing
        Ok(())
    }

    /// Copy vm-pkg binary to container
    fn copy_vm_pkg_binary(&self) -> Result<()> {
        let container = self.container_name();

        // Find vm-pkg binary in our build directory
        let tool_dir = vm_config::get_tool_dir();

        // Try platform-specific path first
        let platform = format!("{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH.replace("aarch64", "aarch64").replace("x86_64", "x86_64"));
        let platform_vm_pkg = tool_dir.join(format!("rust/target/{}/release/vm-pkg", platform));

        let vm_pkg_path = if platform_vm_pkg.exists() {
            platform_vm_pkg
        } else {
            // Fallback to generic release path
            tool_dir.join("rust/target/release/vm-pkg")
        };

        if vm_pkg_path.exists() {
            // Copy the binary to container (follow symlinks with -L)
            let output = std::process::Command::new("docker")
                .args(["cp", "-L", &vm_pkg_path.to_string_lossy(),
                       &format!("{}:/vm-tool/rust/target/release/vm-pkg", container)])
                .output()?;

            if !output.status.success() {
                eprintln!("Warning: Failed to copy vm-pkg binary: {}",
                    String::from_utf8_lossy(&output.stderr));
            } else {
                // Make it executable
                std::process::Command::new("docker")
                    .args(["exec", &container, "sudo", "chmod", "+x",
                           "/vm-tool/rust/target/release/vm-pkg"])
                    .output()?;
            }
        } else {
            eprintln!("Warning: vm-pkg binary not found at expected locations");
        }

        Ok(())
    }

    /// Write content to a file in the container
    fn write_to_container(&self, path: &str, content: &str) -> Result<()> {
        use base64::Engine;
        let container = self.container_name();
        let encoded_content = base64::engine::general_purpose::STANDARD.encode(content);

        // Use base64 to safely transfer content with special characters (with sudo)
        let decode_cmd = format!("echo '{}' | base64 -d | sudo tee '{}' > /dev/null", encoded_content, path);

        let output = std::process::Command::new("docker")
            .args(["exec", &container, "sh", "-c", &decode_cmd])
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to write {}: {}", path,
                String::from_utf8_lossy(&output.stderr)));
        }

        Ok(())
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
        if let Err(_) = std::process::Command::new("docker").arg("ps").output() {
            eprintln!("⚠️  Docker daemon may not be responding properly");
            eprintln!("   Try: docker system prune -f");
        }

        Ok(())
    }

    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("docker") {
            return Err(ProviderError::DependencyNotFound("Docker".to_string()).into());
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
                return Ok(())
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
        let template_path = vm_config::get_tool_dir()
            .join("rust/vm-provider/src/docker/template.yml");

        if template_path.exists() {
            eprintln!("Loading Docker Compose template from: {}", template_path.display());
            std::fs::read_to_string(&template_path)
                .with_context(|| format!("Failed to read template from {:?}", template_path))
        } else {
            // Fallback to embedded template for production
            eprintln!("Using embedded Docker Compose template");
            Ok(include_str!("docker/template.yml").to_string())
        }
    }

    fn render_docker_compose(&self) -> Result<String> {
        let mut tera = Tera::default();
        let template_content = Self::load_docker_compose_template()?;
        tera.add_raw_template("docker-compose.yml", &template_content)?;

        let project_dir_str = Self::path_to_string(&self._project_dir)?;
        let vm_tool_dir_str = "/vm-tool";

        let current_uid = vm_config::get_current_uid();
        let current_gid = vm_config::get_current_gid();
        let project_user = self.config.vm.as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

        let mut context = TeraContext::new();
        context.insert("config", &self.config);
        context.insert("project_dir", &project_dir_str);
        context.insert("vm_tool_dir", &vm_tool_dir_str);
        context.insert("project_uid", &current_uid.to_string());
        context.insert("project_gid", &current_gid.to_string());
        context.insert("project_user", &project_user);
        context.insert("is_macos", &cfg!(target_os = "macos"));

        let content = tera.render("docker-compose.yml", &context)?;
        Ok(content)
    }

    fn write_docker_compose(&self) -> Result<PathBuf> {
        let content = self.render_docker_compose()?;

        let path = self.temp_dir.join("docker-compose.yml");
        fs::write(&path, content.as_bytes())?;

        Ok(path)
    }

    fn write_dockerfile(&self) -> Result<PathBuf> {
        let dockerfile_path = self._project_dir.join("Dockerfile");

        // Use the new Tera template for Dockerfile
        let mut tera = Tera::default();
        let template_content = include_str!("docker/Dockerfile.j2");
        tera.add_raw_template("Dockerfile", template_content)?;

        let current_uid = vm_config::get_current_uid();
        let current_gid = vm_config::get_current_gid();
        let project_user = self.config.vm.as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

        let mut context = TeraContext::new();
        context.insert("config", &self.config);
        context.insert("project_uid", &current_uid.to_string());
        context.insert("project_gid", &current_gid.to_string());
        context.insert("project_user", &project_user);

        let content = tera.render("Dockerfile", &context)?;
        fs::write(&dockerfile_path, content.as_bytes())?;

        Ok(dockerfile_path)
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
        MacOSAudioManager::setup()?;

        println!("🐳 Creating Docker Environment");
        print!("📄 Generating docker-compose.yml... ");
        let compose_path = self.write_docker_compose()?;
        println!("✅");

        print!("📄 Generating Dockerfile... ");
        self.write_dockerfile()?;
        println!("✅");

        println!("\n🔨 Building container image (this may take a while)...");
        stream_docker_build(&[
            "compose",
            "-f",
            Self::path_to_string(&compose_path)?,
            "build",
        ])
        .context("Docker build failed")?;
        println!("✅ Build complete");

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

        println!("\n🔧 Provisioning environment");
        print!("⏳ Waiting for container readiness... ");
        let max_attempts = 30;
        let mut attempt = 1;
        while attempt <= max_attempts {
            if let Ok(output) = std::process::Command::new("docker").args(["exec", &self.container_name(), "echo", "ready"]).output() {
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
        let copy_result = std::process::Command::new("docker").args(["cp", Self::path_to_string(&temp_config_path)?, &format!("{}:/tmp/vm-config.json", self.container_name())]).output()?;
        if !copy_result.status.success() {
            println!("❌");
            return Err(anyhow::anyhow!("Failed to copy config to container"));
        }
        println!("✅");

        print!("📦 Configuration ready in container... ");
        println!("✅");

        println!("🎭 Running Ansible provisioning...");
        stream_command(
            "docker",
            &["exec", &self.container_name(), "bash", "-c", "ansible-playbook -i localhost, -c local /embedded-ansible/playbook.yml"],
        ).context("Ansible provisioning failed")?;
        println!("✅ Provisioning complete");

        print!("🔄 Starting services... ");
        if std::process::Command::new("docker").args(["exec", &self.container_name(), "bash", "-c", "supervisorctl reread && supervisorctl update"]).output().is_ok() {
            println!("✅");
        } else {
            println!("⚠️ (services may start later)");
        }

        println!("\n✅ Docker environment created successfully!");
        Ok(())
    }

    fn start(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            self.write_docker_compose()?;
        }
        stream_command("docker", &["compose", "-f", Self::path_to_string(&compose_path)?, "up", "-d"])
            .context("Failed to start container with docker-compose")
    }

    fn stop(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if compose_path.exists() {
            stream_command("docker", &["compose", "-f", Self::path_to_string(&compose_path)?, "stop"])
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
        stream_command("docker", &["compose", "-f", Self::path_to_string(&compose_path)?, "down", "--volumes"])?;
        MacOSAudioManager::cleanup()?;
        Ok(())
    }

    fn ssh(&self, relative_path: &Path) -> Result<()> {
        let workspace_path = self.config.project.as_ref().and_then(|p| p.workspace_path.as_deref()).unwrap_or("/workspace");
        let project_user = self.config.vm.as_ref().and_then(|vm| vm.user.as_deref()).unwrap_or("developer");
        let shell = self.config.terminal.as_ref().and_then(|t| t.shell.as_deref()).unwrap_or("zsh");

        let target_path = SecurityValidator::validate_relative_path(relative_path, workspace_path)?;
        let target_dir = target_path.to_string_lossy().to_string();

        let tty_flag = if atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stdout) { "-it" } else { "-i" };

        let status = duct::cmd("docker", &["exec", tty_flag, "-e", &format!("VM_TARGET_DIR={}", target_dir), &self.container_name(), "sudo", "-u", project_user, "sh", "-c", &format!("cd \"$VM_TARGET_DIR\" && exec {}", shell)]).run()?.status;

        if !status.success() {
            return Err(ProviderError::CommandFailed("SSH command failed".to_string()).into());
        }
        Ok(())
    }

    fn exec(&self, cmd: &[String]) -> Result<()> {
        let container = self.container_name();
        let workspace_path = self.config.project.as_ref().and_then(|p| p.workspace_path.as_deref()).unwrap_or("/workspace");
        let project_user = self.config.vm.as_ref().and_then(|vm| vm.user.as_deref()).unwrap_or("developer");

        let mut args: Vec<&str> = vec!["exec", "-w", workspace_path, "--user", project_user, &container];
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
            stream_command("docker", &["compose", "-f", Self::path_to_string(&compose_path)?, "ps"])?;
        } else {
            stream_command("docker", &["ps", "-f", &format!("name={}", self.container_name())])?;
        }

        if let Some(port_range) = &self.config.port_range {
            println!("\n📡 Port Range: {}", port_range);
            if !self.config.ports.is_empty() {
                if let Ok(parsed_range) = vm_ports::PortRange::parse(port_range) {
                    let (start, end) = (parsed_range.start, parsed_range.end);
                    let used_count = self.config.ports.values().filter(|&&p| p >= start && p <= end).count();
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
        let status_output = std::process::Command::new("docker").args(["inspect", "--format", "{{.State.Status}}", &container]).output()?;
        let status = String::from_utf8_lossy(&status_output.stdout).trim().to_string();
        if status != "running" {
            return Err(anyhow::anyhow!("Container {} is not running. Start it first with 'vm start'", container));
        }

        let progress = ProgressReporter::new();
        progress.phase_header("🔧", "PROVISIONING PHASE");
        progress.subtask("├─", "Re-running Ansible provisioning...");

        let config_json = self.config.to_json()?;
        let temp_config_path = self.temp_dir.join("vm-config.json");
        std::fs::write(&temp_config_path, config_json)?;
        stream_command("docker", &["cp", Self::path_to_string(&temp_config_path)?, &format!("{}:/tmp/vm-config.json", self.container_name())])?;

        let ansible_result = stream_command("docker", &["exec", &self.container_name(), "bash", "-c", "ansible-playbook -i localhost, -c local /vm-tool/shared/ansible/playbook.yml"]);
        if ansible_result.is_err() {
            progress.error("└─", "Provisioning failed");
            return Err(anyhow::anyhow!("Ansible provisioning failed"));
        }
        progress.complete("└─", "Provisioning complete");
        Ok(())
    }

    fn list(&self) -> Result<()> {
        let project_name = self.project_name();
        stream_command("docker", &["ps", "-a", "--filter", &format!("name={}", project_name)])
            .context("Failed to list containers")
    }

    fn kill(&self) -> Result<()> {
        stream_command("docker", &["kill", &self.container_name()])
            .context("Failed to kill container")
    }

    fn get_sync_directory(&self) -> Result<String> {
        Ok(self.config.project.as_ref().and_then(|p| p.workspace_path.as_deref()).unwrap_or("/workspace").to_string())
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
        stream_command("docker", &["compose", "-f", Self::path_to_string(&compose_path)?, "up", "-d"])?;

        progress.task(&main_phase, "Checking container health...");
        if !self.check_container_health(&state.container_name)? {
            progress.finish_phase(&main_phase, "Mount update failed - container not healthy");
            return Err(anyhow::anyhow!("Container is not healthy after mount update"));
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
            project.name = Some("vm-temp".to_string());
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
        let output = std::process::Command::new("docker").args(["inspect", "--format", "{{.State.Status}}", container_name]).output()?;
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
