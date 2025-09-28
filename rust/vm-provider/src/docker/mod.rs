// Docker provider implementation split into logical modules

pub mod build;
pub mod command;
pub mod compose;
pub mod host_packages;
pub mod lifecycle;

// Re-export the main types and functions for backwards compatibility
pub use build::BuildOperations;
pub use command::DockerOps;
pub use lifecycle::LifecycleOperations;

// Standard library
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// External crates
use anyhow::{Context, Result};
use tera::Tera;

// Internal imports
use crate::{error::ProviderError, preflight, Provider, TempProvider};
use vm_common::command_stream::is_tool_installed;
use vm_config::config::VmConfig;

/// Container user and permission configuration
#[derive(Debug, Clone)]
pub struct UserConfig {
    pub uid: u32,
    pub gid: u32,
    pub username: String,
}

impl UserConfig {
    /// Extract user configuration from VM config
    pub fn from_vm_config(config: &VmConfig) -> Self {
        let current_uid = vm_config::get_current_uid();
        let current_gid = vm_config::get_current_gid();
        let project_user = config
            .vm
            .as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

        Self {
            uid: current_uid,
            gid: current_gid,
            username: project_user.to_string(),
        }
    }
}

/// Helper for building docker-compose command arguments
pub struct ComposeCommand;

impl ComposeCommand {
    /// Build docker-compose command arguments with compose file path
    pub fn build_args(
        compose_path: &Path,
        subcommand: &str,
        extra_args: &[&str],
    ) -> Result<Vec<String>, anyhow::Error> {
        let mut args = vec![
            "compose".to_string(),
            "-f".to_string(),
            BuildOperations::path_to_string(compose_path)?.to_string(),
            subcommand.to_string(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));
        Ok(args)
    }
}

pub struct DockerProvider {
    config: VmConfig,
    _project_dir: PathBuf, // The root of the user's project
    temp_dir: PathBuf, // Persistent project-specific directory for generated files like docker-compose.yml
}

impl DockerProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("docker") {
            return Err(ProviderError::DependencyNotFound("Docker".into()).into());
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

    /// Helper to create LifecycleOperations instance
    fn lifecycle_ops(&self) -> LifecycleOperations<'_> {
        LifecycleOperations::new(&self.config, &self.temp_dir, &self._project_dir)
    }
}

/// Shared template engine for Docker compose operations
static COMPOSE_TERA: OnceLock<Tera> = OnceLock::new();

pub(crate) fn get_compose_tera() -> &'static Tera {
    COMPOSE_TERA.get_or_init(|| {
        let mut tera = Tera::default();
        tera.add_raw_template("docker-compose.yml", include_str!("template.yml"))
            .expect("Failed to add docker-compose template");
        tera
    })
}

/// Shared template engine for Dockerfile operations
static DOCKERFILE_TERA: OnceLock<Tera> = OnceLock::new();

pub(crate) fn get_dockerfile_tera() -> &'static Tera {
    DOCKERFILE_TERA.get_or_init(|| {
        let mut tera = Tera::default();
        tera.add_raw_template("Dockerfile", include_str!("Dockerfile.j2"))
            .expect("Failed to add Dockerfile template");
        tera
    })
}

/// Shared template engine for temporary VM docker-compose operations
static TEMP_COMPOSE_TERA: OnceLock<Tera> = OnceLock::new();

pub(crate) fn get_temp_compose_tera() -> &'static Tera {
    TEMP_COMPOSE_TERA.get_or_init(|| {
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
    {% if config.security.enable_debugging | default(value=false) %}
    cap_add:
      - SYS_PTRACE
    {% endif %}
    security_opt:
      {% if config.security.enable_debugging | default(value=false) %}
      - seccomp=unconfined
      {% endif %}
      {% if config.security.no_new_privileges | default(value=true) %}
      - no-new-privileges
      {% endif %}

volumes:
  vmtemp_nvm:
  vmtemp_cache:
"#;
        let mut tera = Tera::default();
        tera.add_raw_template("docker-compose.yml", TEMP_DOCKER_COMPOSE_TEMPLATE)
            .expect("Failed to add temporary docker-compose template");
        tera
    })
}

impl Drop for DockerProvider {
    fn drop(&mut self) {
        if self.temp_dir.exists() {
            // Best-effort cleanup. Ignore errors if cleanup fails.
            let _ = fs::remove_dir_all(&self.temp_dir);
        }
    }
}

impl Provider for DockerProvider {
    fn name(&self) -> &'static str {
        "docker"
    }

    fn create(&self) -> Result<()> {
        preflight::check_system_resources()?;

        let lifecycle = self.lifecycle_ops();
        lifecycle.create_container()
    }

    fn create_instance(&self, instance_name: &str) -> Result<()> {
        preflight::check_system_resources()?;

        let lifecycle = self.lifecycle_ops();
        lifecycle.create_container_with_instance(instance_name)
    }

    fn start(&self, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.start_container(container)
    }

    fn stop(&self, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.stop_container(container)
    }

    fn destroy(&self, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.destroy_container(container)
    }

    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.ssh_into_container(container, relative_path)
    }

    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.exec_in_container(container, cmd)
    }

    fn logs(&self, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.show_logs(container)
    }

    fn status(&self, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.show_status(container)
    }

    fn restart(&self, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.restart_container(container)
    }

    fn provision(&self, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.provision_existing(container)
    }

    fn list(&self) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.list_containers()
    }

    fn kill(&self, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.kill_container(container)
    }

    fn get_sync_directory(&self) -> String {
        let lifecycle = self.lifecycle_ops();
        lifecycle.get_sync_directory()
    }

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
    }

    fn supports_multi_instance(&self) -> bool {
        true
    }

    fn resolve_instance_name(&self, instance: Option<&str>) -> Result<String> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.resolve_target_container(instance)
    }

    fn list_instances(&self) -> Result<Vec<crate::InstanceInfo>> {
        use crate::common::instance::create_docker_instance_info_with_metadata;

        // Parse docker ps output to get instance info including creation time and uptime
        let output = std::process::Command::new("docker")
            .args([
                "ps",
                "-a",
                "--format",
                "{{.Names}}\t{{.ID}}\t{{.Status}}\t{{.CreatedAt}}\t{{.RunningFor}}",
            ])
            .output()
            .with_context(|| "Failed to list containers for instance information")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Docker container listing failed while gathering instance information"
            ));
        }

        let containers_output = String::from_utf8_lossy(&output.stdout);
        let mut instances = Vec::new();
        let lifecycle = self.lifecycle_ops();
        let project_name = lifecycle.project_name();

        // Filter for project containers
        for line in containers_output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                let name = parts[0];
                let id = parts[1];
                let status = parts[2];
                let created_at = parts[3];
                let running_for = parts[4];

                // Only include containers that belong to this project
                if name.starts_with(&format!("{}-", project_name)) || name == project_name {
                    instances.push(create_docker_instance_info_with_metadata(
                        name,
                        id,
                        status,
                        Some(created_at),
                        Some(running_for),
                    ));
                }
            }
        }

        Ok(instances)
    }
}

impl TempProvider for DockerProvider {
    fn update_mounts(&self, state: &crate::TempVmState) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.update_mounts(state)
    }

    fn recreate_with_mounts(&self, state: &crate::TempVmState) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.recreate_with_mounts(state)
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.check_container_health(container_name)
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.is_container_running(container_name)
    }
}
