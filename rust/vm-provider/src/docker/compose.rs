// Standard library
use std::fs;
use std::path::{Path, PathBuf};

// External crates
use anyhow::{Context, Result};
use tera::Context as TeraContext;

// Internal imports
use super::build::BuildOperations;
use super::host_packages::{
    detect_packages, get_package_env_vars, get_volume_mounts, PackageManager,
};
use super::{ComposeCommand, DockerOps, UserConfig};
use crate::TempVmState;
use vm_common::command_stream::stream_command;
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

    pub fn render_docker_compose(&self, build_context_dir: &Path) -> Result<String> {
        // Use shared template engine instead of creating new instance
        let tera = super::get_compose_tera();

        let project_dir_str = BuildOperations::path_to_string(self.project_dir)?;
        let build_context_str = BuildOperations::path_to_string(build_context_dir)?;

        let user_config = UserConfig::from_vm_config(self.config);

        // Detect host package locations for mounting (only if package linking is enabled)
        let mut host_info = super::host_packages::HostPackageInfo::new();

        // Check pip packages only if pip linking is enabled
        if self.config.package_linking.as_ref().is_some_and(|p| p.pip)
            && !self.config.pip_packages.is_empty()
        {
            let pip_info = detect_packages(&self.config.pip_packages, PackageManager::Pip);
            host_info.pip_site_packages = pip_info.pip_site_packages;
            host_info.pipx_base_dir = pip_info.pipx_base_dir;

            // Include all detected pip packages for host mounting
            host_info
                .detected_packages
                .extend(pip_info.detected_packages);
        }

        // Check npm packages only if npm linking is enabled
        if self.config.package_linking.as_ref().is_some_and(|p| p.npm)
            && !self.config.npm_packages.is_empty()
        {
            let npm_info = detect_packages(&self.config.npm_packages, PackageManager::Npm);
            host_info.npm_global_dir = npm_info.npm_global_dir;
            host_info.npm_local_dir = npm_info.npm_local_dir;
            host_info
                .detected_packages
                .extend(npm_info.detected_packages);
        }

        // Check cargo packages only if cargo linking is enabled
        if self
            .config
            .package_linking
            .as_ref()
            .is_some_and(|p| p.cargo)
            && !self.config.cargo_packages.is_empty()
        {
            let cargo_info = detect_packages(&self.config.cargo_packages, PackageManager::Cargo);
            host_info.cargo_registry = cargo_info.cargo_registry;
            host_info.cargo_bin = cargo_info.cargo_bin;
            host_info
                .detected_packages
                .extend(cargo_info.detected_packages);
        }

        // Get volume mounts and environment variables
        let host_mounts = get_volume_mounts(&host_info);
        let host_env_vars = get_package_env_vars(&host_info);

        let mut context = TeraContext::new();
        context.insert("config", &self.config);
        context.insert("project_dir", &project_dir_str);
        context.insert("build_context_dir", &build_context_str);
        context.insert("project_uid", &user_config.uid.to_string());
        context.insert("project_gid", &user_config.gid.to_string());
        context.insert("project_user", &user_config.username);
        context.insert("is_macos", &cfg!(target_os = "macos"));
        context.insert("host_mounts", &host_mounts);
        context.insert("host_env_vars", &host_env_vars);
        // No local package mounts or environment variables needed
        let local_pipx_mounts: Vec<(String, String)> = Vec::new();
        let local_env_vars: Vec<(String, String)> = Vec::new();

        context.insert("local_pipx_mounts", &local_pipx_mounts);
        context.insert("local_env_vars", &local_env_vars);

        let content = tera.render("docker-compose.yml", &context)?;
        Ok(content)
    }

    pub fn write_docker_compose(&self, build_context_dir: &Path) -> Result<PathBuf> {
        let content = self.render_docker_compose(build_context_dir)?;

        let path = self.temp_dir.join("docker-compose.yml");
        fs::write(&path, content.as_bytes())?;

        Ok(path)
    }

    /// Write docker-compose.yml with custom instance name
    pub fn write_docker_compose_with_instance(
        &self,
        build_context_dir: &Path,
        instance_name: &str,
    ) -> Result<PathBuf> {
        let content = self.render_docker_compose_with_instance(build_context_dir, instance_name)?;

        let path = self.temp_dir.join("docker-compose.yml");
        fs::write(&path, content.as_bytes())?;

        Ok(path)
    }

    /// Render docker-compose.yml with custom instance name
    pub fn render_docker_compose_with_instance(
        &self,
        build_context_dir: &Path,
        instance_name: &str,
    ) -> Result<String> {
        // Use shared template engine instead of creating new instance
        let tera = super::get_compose_tera();

        let project_dir_str = BuildOperations::path_to_string(self.project_dir)?;
        let build_context_str = BuildOperations::path_to_string(build_context_dir)?;

        let user_config = UserConfig::from_vm_config(self.config);

        // Detect host package locations for mounting (only if package linking is enabled)
        let mut host_info = super::host_packages::HostPackageInfo::new();

        // Check pip packages only if pip linking is enabled
        if self.config.package_linking.as_ref().is_some_and(|p| p.pip)
            && !self.config.pip_packages.is_empty()
        {
            let pip_info = detect_packages(&self.config.pip_packages, PackageManager::Pip);
            host_info.pip_site_packages = pip_info.pip_site_packages;
            host_info.pipx_base_dir = pip_info.pipx_base_dir;

            // Include all detected pip packages for host mounting
            host_info
                .detected_packages
                .extend(pip_info.detected_packages);
        }

        // Check npm packages only if npm linking is enabled
        if self.config.package_linking.as_ref().is_some_and(|p| p.npm)
            && !self.config.npm_packages.is_empty()
        {
            let npm_info = detect_packages(&self.config.npm_packages, PackageManager::Npm);
            host_info.npm_global_dir = npm_info.npm_global_dir;
            host_info.npm_local_dir = npm_info.npm_local_dir;
            host_info
                .detected_packages
                .extend(npm_info.detected_packages);
        }

        // Check cargo packages only if cargo linking is enabled
        if self
            .config
            .package_linking
            .as_ref()
            .is_some_and(|p| p.cargo)
            && !self.config.cargo_packages.is_empty()
        {
            let cargo_info = detect_packages(&self.config.cargo_packages, PackageManager::Cargo);
            host_info.cargo_registry = cargo_info.cargo_registry;
            host_info.cargo_bin = cargo_info.cargo_bin;
            host_info
                .detected_packages
                .extend(cargo_info.detected_packages);
        }

        // Get volume mounts and environment variables
        let host_mounts = get_volume_mounts(&host_info);
        let host_env_vars = get_package_env_vars(&host_info);

        // Create a custom config with the instance name
        let mut custom_config = self.config.clone();
        if let Some(ref mut project) = custom_config.project {
            if let Some(ref project_name) = project.name {
                project.name = Some(format!("{}-{}", project_name, instance_name));
            } else {
                project.name = Some(format!("vm-project-{}", instance_name));
            }
        } else {
            custom_config.project = Some(vm_config::config::ProjectConfig {
                name: Some(format!("vm-project-{}", instance_name)),
                ..Default::default()
            });
        }

        let mut context = TeraContext::new();
        context.insert("config", &custom_config);
        context.insert("project_dir", &project_dir_str);
        context.insert("build_context_dir", &build_context_str);
        context.insert("project_uid", &user_config.uid.to_string());
        context.insert("project_gid", &user_config.gid.to_string());
        context.insert("project_user", &user_config.username);
        context.insert("is_macos", &cfg!(target_os = "macos"));
        context.insert("host_mounts", &host_mounts);
        context.insert("host_env_vars", &host_env_vars);
        // No local package mounts or environment variables needed
        let local_pipx_mounts: Vec<(String, String)> = Vec::new();
        let local_env_vars: Vec<(String, String)> = Vec::new();

        context.insert("local_pipx_mounts", &local_pipx_mounts);
        context.insert("local_env_vars", &local_env_vars);

        let content = tera.render("docker-compose.yml", &context)?;
        Ok(content)
    }

    pub fn render_docker_compose_with_mounts(&self, state: &TempVmState) -> Result<String> {
        // Use shared template engine instead of creating new instance
        let tera = super::get_temp_compose_tera();

        let mut context = TeraContext::new();
        context.insert("config", &self.config);
        context.insert("container_name", &state.container_name);
        context.insert("mounts", &state.mounts);

        let content = tera.render("docker-compose.yml", &context)?;
        Ok(content)
    }

    #[allow(dead_code)]
    pub fn start_with_compose(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            // Fallback: prepare build context and generate compose file
            let build_ops = BuildOperations::new(self.config, self.temp_dir);
            let build_context = build_ops.prepare_build_context()?;
            self.write_docker_compose(&build_context)?;
        }

        // Check if the container already exists (stopped or running)
        let container_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .map(|s| format!("{}-dev", s))
            .unwrap_or_else(|| "vm-project-dev".to_string());

        let container_exists = DockerOps::container_exists(&container_name).unwrap_or(false);

        // Use 'start' if container exists, 'up -d' if it doesn't
        let (command, extra_args) = if container_exists {
            ("start", vec![])
        } else {
            ("up", vec!["-d"])
        };

        let args = ComposeCommand::build_args(&compose_path, command, &extra_args)?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command("docker", &args_refs)
            .with_context(|| "Failed to start container using docker-compose. Check that docker-compose.yml is valid and Docker is running")
    }

    #[allow(dead_code)]
    pub fn stop_with_compose(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if compose_path.exists() {
            let args = ComposeCommand::build_args(&compose_path, "stop", &[])?;
            let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            stream_command("docker", &args_refs)
                .with_context(|| "Failed to stop container using docker-compose. Container may already be stopped or docker-compose may be inaccessible")
        } else {
            Err(anyhow::anyhow!(
                "docker-compose.yml not found in '{}'. Cannot stop container without compose configuration",
                self.temp_dir.display()
            ))
        }
    }

    #[allow(dead_code)]
    pub fn destroy_with_compose(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            return Err(anyhow::anyhow!(
                "docker-compose.yml not found in '{}' for container destruction. Use direct Docker commands instead",
                self.temp_dir.display()
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
            Err(anyhow::anyhow!(
                "docker-compose.yml not found in '{}'. Cannot show container status without compose configuration",
                self.temp_dir.display()
            ))
        }
    }
}
