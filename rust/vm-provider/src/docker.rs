use crate::{
    error::ProviderError,
    progress::ProgressReporter,
    utils::{is_tool_installed, stream_command},
    Provider,
};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tera::{Tera, Context as TeraContext};
use vm_config::config::VmConfig;
use vm_temp::{TempVmState, TempProvider};

const DOCKER_COMPOSE_TEMPLATE: &str = include_str!("docker/template.yml");
const DOCKERFILE_TEMPLATE: &str = include_str!("../../../providers/docker/Dockerfile");

pub struct DockerProvider {
    config: VmConfig,
    _project_dir: PathBuf, // The root of the user's project
    temp_dir: PathBuf,    // For storing generated files like docker-compose.yml
}

impl DockerProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("docker") {
            return Err(ProviderError::DependencyNotFound("Docker".to_string()).into());
        }

        let project_dir = std::env::current_dir()?;
        let temp_dir = project_dir.join(".vm-tmp");
        fs::create_dir_all(&temp_dir)?;

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
            "Docker daemon is not running. Please start Docker Desktop or the Docker service.".to_string()
        ).into())
    }

    fn project_name(&self) -> &str {
        self.config.project.as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project")
    }

    fn container_name(&self) -> String {
        format!("{}-dev", self.project_name())
    }

    fn render_docker_compose(&self) -> Result<String> {
        let mut tera = Tera::default();
        tera.add_raw_template("docker-compose.yml", DOCKER_COMPOSE_TEMPLATE)?;

        let mut context = TeraContext::new();
        context.insert("config", &self.config);

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
        // Use the embedded Dockerfile template
        let dockerfile_path = self._project_dir.join("Dockerfile");

        // Check if Dockerfile already exists
        if !dockerfile_path.exists() {
            fs::write(&dockerfile_path, DOCKERFILE_TEMPLATE.as_bytes())?;
        }

        Ok(dockerfile_path)
    }
}

impl Provider for DockerProvider {
    fn name(&self) -> &'static str {
        "docker"
    }

    fn create(&self) -> Result<()> {
        self.check_daemon_is_running()?;
        let progress = ProgressReporter::new();
        let main_phase = progress.start_phase("Creating Docker Environment");

        progress.task(&main_phase, "Generating docker-compose.yml...");
        let compose_path = self.write_docker_compose().context("Failed to generate docker-compose.yml")?;

        progress.task(&main_phase, "Generating Dockerfile...");
        let _dockerfile_path = self.write_dockerfile().context("Failed to generate Dockerfile")?;
        progress.finish_phase(&main_phase, "Done.");

        let build_phase = progress.start_phase("Building container image");
        progress.task(&build_phase, "Running `docker compose build`...");
        stream_command(
            "docker",
            &["compose", "-f", compose_path.to_str().unwrap(), "build"],
        ).context("Docker build failed")?;
        progress.finish_phase(&build_phase, "Build complete.");

        let up_phase = progress.start_phase("Starting containers");
        progress.task(&up_phase, "Running `docker compose up -d`...");
        stream_command(
            "docker",
            &["compose", "-f", compose_path.to_str().unwrap(), "up", "-d"],
        ).context("Docker up failed")?;
        progress.finish_phase(&up_phase, "Containers started.");

        // Wait for container to be ready
        let provisioning_phase = progress.start_phase("Provisioning environment");
        progress.task(&provisioning_phase, "Waiting for container readiness...");

        let max_attempts = 30;
        let mut attempt = 1;
        let mut container_ready = false;

        while attempt <= max_attempts {
            if stream_command("docker", &["exec", &self.container_name(), "echo", "ready"]).is_ok() {
                container_ready = true;
                break;
            }

            if attempt == max_attempts {
                return Err(anyhow::anyhow!("Container failed to become ready after {} attempts", max_attempts));
            }

            std::thread::sleep(std::time::Duration::from_secs(2));
            attempt += 1;
        }

        if !container_ready {
            return Err(anyhow::anyhow!("Container initialization timed out"));
        }

        progress.task(&provisioning_phase, "Container ready.");

        // Copy config to container for Ansible
        progress.task(&provisioning_phase, "Loading project configuration...");
        let config_json = self.config.to_json().context("Failed to serialize config")?;
        let temp_config_path = self.temp_dir.join("vm-config.json");
        fs::write(&temp_config_path, config_json)?;

        stream_command("docker", &[
            "cp",
            temp_config_path.to_str().unwrap(),
            &format!("{}:/tmp/vm-config.json", self.container_name())
        ]).context("Failed to copy config to container")?;
        progress.task(&provisioning_phase, "Configuration loaded.");

        // Run Ansible provisioning
        progress.task(&provisioning_phase, "Running Ansible provisioning...");
        let ansible_result = stream_command("docker", &[
            "exec",
            &self.container_name(),
            "bash", "-c",
            "ansible-playbook -i localhost, -c local /vm-tool/shared/ansible/playbook.yml"
        ]);

        if ansible_result.is_err() {
            progress.task(&provisioning_phase, "Provisioning failed.");
            return Err(anyhow::anyhow!("Ansible provisioning failed"));
        }
        progress.task(&provisioning_phase, "Provisioning complete.");

        // Start supervisor services
        progress.task(&provisioning_phase, "Starting services...");
        let _ = stream_command("docker", &[
            "exec",
            &self.container_name(),
            "bash", "-c",
            "supervisorctl reread && supervisorctl update"
        ]);
        progress.task(&provisioning_phase, "Services started.");

        progress.finish_phase(&provisioning_phase, "Environment ready.");

        println!("\n✅ Docker environment created successfully!");
        Ok(())
    }

    fn start(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            self.write_docker_compose()?;
        }

        stream_command(
            "docker",
            &["compose", "-f", compose_path.to_str().unwrap(), "up", "-d"],
        ).context("Failed to start container with docker-compose")
    }

    fn stop(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if compose_path.exists() {
            stream_command(
                "docker",
                &["compose", "-f", compose_path.to_str().unwrap(), "stop"],
            ).context("Failed to stop container with docker-compose")
        } else {
            // Fallback to direct container stop if compose file doesn't exist
            stream_command(
                "docker",
                &["stop", &self.container_name()],
            ).context("Failed to stop container")
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
            &["compose", "-f", compose_path.to_str().unwrap(), "down", "--volumes"],
        )
    }

    fn ssh(&self, relative_path: &Path) -> Result<()> {
        // Get workspace path and user from config
        let workspace_path = self.config.project.as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace");
        let project_user = self.config.vm.as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

        // Get shell from config (terminal.shell or default to zsh)
        let shell = self.config.terminal.as_ref()
            .and_then(|t| t.shell.as_deref())
            .unwrap_or("zsh");

        // Calculate target directory
        let target_dir = if relative_path.as_os_str().is_empty() || relative_path == Path::new(".") {
            workspace_path.to_string()
        } else {
            format!("{}/{}", workspace_path, relative_path.display())
        };

        // Run interactive shell with proper working directory
        let status = duct::cmd(
            "docker",
            &[
                "exec", "-it", &self.container_name(),
                "sudo", "-u", project_user,
                "sh", "-c",
                &format!("VM_TARGET_DIR='{}' exec {}", target_dir, shell)
            ],
        )
        .run()?
        .status;

        if !status.success() {
            return Err(ProviderError::CommandFailed(
                "SSH command failed".to_string(),
            ).into());
        }
        Ok(())
    }

    fn exec(&self, cmd: &[String]) -> Result<()> {
        let container = self.container_name();
        let workspace_path = self.config.project.as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace");
        let project_user = self.config.vm.as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

        let mut args: Vec<&str> = vec!["exec", &container, "su", "-", project_user, "-c"];

        // Build command with working directory context
        let cmd_str = cmd.join(" ");
        let full_cmd = format!("cd {} && {}", workspace_path, cmd_str);
        args.push(&full_cmd);

        stream_command("docker", &args)
    }

    fn logs(&self) -> Result<()> {
        stream_command("docker", &["logs", "-f", &self.container_name()])
    }

    fn status(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");

        if compose_path.exists() {
            // Use docker-compose ps for formatted status
            stream_command(
                "docker",
                &["compose", "-f", compose_path.to_str().unwrap(), "ps"],
            ).context("Failed to get container status with docker-compose")?;
        } else {
            // Fallback to docker ps with filter for this container
            stream_command(
                "docker",
                &["ps", "-f", &format!("name={}", self.container_name())],
            ).context("Failed to get container status")?;
        }

        // Show port range info if configured
        if let Some(port_range) = &self.config.port_range {
            println!("\n📡 Port Range: {}", port_range);

            // Count used ports in range if configured
            if !self.config.ports.is_empty() {
                let range_parts: Vec<&str> = port_range.split('-').collect();
                if range_parts.len() == 2 {
                    if let (Ok(range_start), Ok(range_end)) = (
                        range_parts[0].parse::<u16>(),
                        range_parts[1].parse::<u16>()
                    ) {
                        let used_count = self.config.ports.values()
                            .filter(|&port| *port >= range_start && *port <= range_end)
                            .count();
                        let total_ports = (range_end - range_start + 1) as usize;
                        println!("   Used: {}/{} ports", used_count, total_ports);
                    }
                }
            }
        }

        Ok(())
    }

    fn restart(&self) -> Result<()> {
        // Stop then start with compose
        self.stop()?;
        self.start()
    }

    fn provision(&self) -> Result<()> {
        // Re-run ansible without recreating container
        let container = self.container_name();

        // Check if container is running
        let status_output = std::process::Command::new("docker")
            .args(&["inspect", "--format", "{{.State.Status}}", &container])
            .output()?;

        let status = String::from_utf8_lossy(&status_output.stdout).trim().to_string();
        if status != "running" {
            return Err(anyhow::anyhow!("Container {} is not running. Start it first with 'vm start'", container));
        }

        let progress = ProgressReporter::new();
        progress.phase_header("🔧", "PROVISIONING PHASE");
        progress.subtask("├─", "Re-running Ansible provisioning...");

        // Copy config to container for Ansible
        let config_json = self.config.to_json().context("Failed to serialize config")?;
        let temp_config_path = self.temp_dir.join("vm-config.json");
        std::fs::write(&temp_config_path, config_json)?;

        stream_command("docker", &[
            "cp",
            temp_config_path.to_str().unwrap(),
            &format!("{}:/tmp/vm-config.json", self.container_name())
        ]).context("Failed to copy config to container")?;

        // Run Ansible provisioning
        let ansible_result = stream_command("docker", &[
            "exec",
            &self.container_name(),
            "bash", "-c",
            "ansible-playbook -i localhost, -c local /vm-tool/shared/ansible/playbook.yml"
        ]);

        if ansible_result.is_err() {
            progress.error("└─", "Provisioning failed");
            return Err(anyhow::anyhow!("Ansible provisioning failed"));
        }

        progress.complete("└─", "Provisioning complete");
        Ok(())
    }

    fn list(&self) -> Result<()> {
        // Use docker ps -a with project filter
        let project_name = self.project_name();
        stream_command(
            "docker",
            &["ps", "-a", "--filter", &format!("name={}", project_name)],
        ).context("Failed to list containers")
    }

    fn kill(&self) -> Result<()> {
        // Force kill the container
        stream_command("docker", &["kill", &self.container_name()])
            .context("Failed to kill container")
    }

    fn get_sync_directory(&self) -> Result<String> {
        // Return workspace_path from config
        let workspace_path = self.config.project.as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace");
        Ok(workspace_path.to_string())
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

        // Check if container is running first
        let is_running = self.is_container_running(&state.container_name)
            .context("Failed to check container status")?;

        if is_running {
            progress.task(&main_phase, "Stopping container...");
            // Stop the container if it's running
            stream_command("docker", &["stop", &state.container_name])
                .context("Failed to stop container")?;
        }

        progress.task(&main_phase, "Recreating container with new mounts...");
        // Recreate the container with new mounts
        self.recreate_with_mounts(state)
            .context("Failed to recreate container with new mounts")?;

        progress.task(&main_phase, "Starting container...");
        // Start the container
        let compose_path = self.temp_dir.join("docker-compose.yml");
        stream_command(
            "docker",
            &["compose", "-f", compose_path.to_str().unwrap(), "up", "-d"],
        ).context("Failed to start container with new mounts")?;

        progress.task(&main_phase, "Checking container health...");
        // Check if container is healthy
        let is_healthy = self.check_container_health(&state.container_name)
            .context("Failed to check container health")?;

        if !is_healthy {
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

        // Create a modified config with the temp VM mounts
        let mut temp_config = self.config.clone();

        // Update the project name to match the temp VM
        if let Some(ref mut project) = temp_config.project {
            project.name = Some("vm-temp".to_string());
        }

        // Generate docker-compose.yml with the new mounts
        let content = self.render_docker_compose_with_mounts(state)
            .context("Failed to render docker-compose.yml with mounts")?;
        let compose_path = self.temp_dir.join("docker-compose.yml");
        fs::write(&compose_path, content.as_bytes())
            .context("Failed to write updated docker-compose.yml")?;

        progress.task(&phase, "Removing old container...");
        // Remove the old container (preserving volumes)
        let _ = stream_command("docker", &["rm", "-f", &state.container_name]);

        progress.finish_phase(&phase, "Container configuration updated");
        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        // Wait for container to be ready
        let max_attempts = 30;
        let mut attempt = 1;

        while attempt <= max_attempts {
            if stream_command("docker", &["exec", container_name, "echo", "ready"]).is_ok() {
                return Ok(true);
            }

            if attempt == max_attempts {
                return Ok(false);
            }

            std::thread::sleep(std::time::Duration::from_secs(2));
            attempt += 1;
        }

        Ok(false)
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        let output = std::process::Command::new("docker")
            .args(&["inspect", "--format", "{{.State.Status}}", container_name])
            .output()
            .context("Failed to check container status")?;

        if !output.status.success() {
            return Ok(false);
        }

        let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(status == "running")
    }
}

impl DockerProvider {
    /// Render docker-compose.yml with custom mounts for temp VMs
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
