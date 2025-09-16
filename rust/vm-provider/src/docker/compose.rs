// Standard library
use std::fs;
use std::path::{Path, PathBuf};

// External crates
use anyhow::{Context, Result};
use tera::{Context as TeraContext, Tera};

// Internal imports
use super::build::BuildOperations;
use super::{UserConfig, ComposeCommand};
use crate::{utils::stream_command, TempVmState};
use vm_config::config::VmConfig;

pub struct ComposeOperations<'a> {
    pub config: &'a VmConfig,
    pub temp_dir: &'a PathBuf,
    pub project_dir: &'a PathBuf,
}

impl<'a> ComposeOperations<'a> {
    pub fn new(config: &'a VmConfig, temp_dir: &'a PathBuf, project_dir: &'a PathBuf) -> Self {
        Self {
            config,
            temp_dir,
            project_dir,
        }
    }

    pub fn load_docker_compose_template() -> Result<String> {
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
            Ok(include_str!("template.yml").into())
        }
    }

    pub fn render_docker_compose(&self, build_context_dir: &Path) -> Result<String> {
        let mut tera = Tera::default();
        let template_content = Self::load_docker_compose_template()?;
        tera.add_raw_template("docker-compose.yml", &template_content)?;

        let project_dir_str = BuildOperations::path_to_string(self.project_dir)?;
        let build_context_str = BuildOperations::path_to_string(build_context_dir)?;

        let user_config = UserConfig::from_vm_config(self.config);

        let mut context = TeraContext::new();
        context.insert("config", &self.config);
        context.insert("project_dir", &project_dir_str);
        context.insert("build_context_dir", &build_context_str);
        context.insert("project_uid", &user_config.uid.to_string());
        context.insert("project_gid", &user_config.gid.to_string());
        context.insert("project_user", &user_config.username);
        context.insert("is_macos", &cfg!(target_os = "macos"));

        let content = tera.render("docker-compose.yml", &context)?;
        Ok(content)
    }

    pub fn write_docker_compose(&self, build_context_dir: &Path) -> Result<PathBuf> {
        let content = self.render_docker_compose(build_context_dir)?;

        let path = self.temp_dir.join("docker-compose.yml");
        fs::write(&path, content.as_bytes())?;

        Ok(path)
    }

    pub fn render_docker_compose_with_mounts(&self, state: &TempVmState) -> Result<String> {
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

    pub fn start_with_compose(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            // Fallback: prepare build context and generate compose file
            let build_ops = BuildOperations::new(self.config, self.temp_dir);
            let build_context = build_ops.prepare_build_context()?;
            self.write_docker_compose(&build_context)?;
        }
        let args = ComposeCommand::build_args(&compose_path, "up", &["-d"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command("docker", &args_refs)
        .context("Failed to start container with docker-compose")
    }

    pub fn stop_with_compose(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if compose_path.exists() {
            let args = ComposeCommand::build_args(&compose_path, "stop", &[])?;
            let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            stream_command("docker", &args_refs)
            .context("Failed to stop container with docker-compose")
        } else {
            Err(anyhow::anyhow!("docker-compose.yml not found"))
        }
    }

    pub fn destroy_with_compose(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            return Err(anyhow::anyhow!(
                "docker-compose.yml not found for destruction"
            ));
        }
        let args = ComposeCommand::build_args(&compose_path, "down", &["--volumes"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command("docker", &args_refs)
    }

    pub fn status_with_compose(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if compose_path.exists() {
            let args = ComposeCommand::build_args(&compose_path, "ps", &[])?;
            let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            stream_command("docker", &args_refs)
        } else {
            Err(anyhow::anyhow!("docker-compose.yml not found"))
        }
    }
}