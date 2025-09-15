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

        // TODO: Add Ansible provisioning step here.

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
            &["start", &self.container_name()],
        ).context("Failed to start container")
    }

    fn stop(&self) -> Result<()> {
        stream_command(
            "docker",
            &["stop", &self.container_name()],
        ).context("Failed to stop container")
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

    fn ssh(&self, _relative_path: &Path) -> Result<()> {
        let status = duct::cmd(
            "docker",
            &["exec", "-it", &self.container_name(), "zsh"],
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
        let mut args: Vec<&str> = vec!["exec", &container];
        // Convert Vec<String> to Vec<&str> for the command
        let cmd_strs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&cmd_strs);
        stream_command("docker", &args)
    }

    fn logs(&self) -> Result<()> {
        stream_command("docker", &["logs", "-f", &self.container_name()])
    }

    fn status(&self) -> Result<()> {
        stream_command(
            "docker",
            &["ps", "-f", &format!("name={}", self.container_name())],
        )
    }
}
