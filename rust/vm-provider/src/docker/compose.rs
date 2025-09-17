// Standard library
use std::fs;
use std::path::{Path, PathBuf};

// External crates
use anyhow::{Context, Result};
use tera::Context as TeraContext;

// Internal imports
use super::build::BuildOperations;
use super::host_packages::{detect_packages, get_package_env_vars, get_volume_mounts, PackageManager};
use super::{ComposeCommand, UserConfig};
use crate::{command_stream::stream_command, TempVmState};
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

    #[allow(dead_code)]
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
        // Use shared template engine instead of creating new instance
        let tera = &super::COMPOSE_TERA;

        let project_dir_str = BuildOperations::path_to_string(self.project_dir)?;
        let build_context_str = BuildOperations::path_to_string(build_context_dir)?;

        let user_config = UserConfig::from_vm_config(self.config);

        // Detect host package locations for mounting (only if package linking is enabled)
        let mut host_info = super::host_packages::HostPackageInfo::new();

        // Check pip packages only if pip linking is enabled
        if self.config.package_linking.as_ref().map_or(false, |p| p.pip) && !self.config.pip_packages.is_empty() {
            if let Ok(pip_info) = detect_packages(&self.config.pip_packages, PackageManager::Pip) {
                host_info.pip_site_packages = pip_info.pip_site_packages;
                host_info.pipx_base_dir = pip_info.pipx_base_dir;

                // Filter out packages that will be handled by local pipx mounting
                let local_pipx_packages: std::collections::HashSet<String> = if let Some(local_pipx) = self.config.extra_config.get("local_pipx_packages") {
                    local_pipx.as_object().map_or_else(|| std::collections::HashSet::new(), |obj| {
                        obj.keys().cloned().collect()
                    })
                } else {
                    std::collections::HashSet::new()
                };

                // Only include packages that are NOT in local_pipx_packages
                for (package_name, location) in pip_info.detected_packages {
                    if !local_pipx_packages.contains(&package_name) {
                        host_info.detected_packages.insert(package_name, location);
                    }
                }
            }
        }

        // Check npm packages only if npm linking is enabled
        if self.config.package_linking.as_ref().map_or(false, |p| p.npm) && !self.config.npm_packages.is_empty() {
            if let Ok(npm_info) = detect_packages(&self.config.npm_packages, PackageManager::Npm) {
                host_info.npm_global_dir = npm_info.npm_global_dir;
                host_info.npm_local_dir = npm_info.npm_local_dir;
                host_info.detected_packages.extend(npm_info.detected_packages);
            }
        }

        // Check cargo packages only if cargo linking is enabled
        if self.config.package_linking.as_ref().map_or(false, |p| p.cargo) && !self.config.cargo_packages.is_empty() {
            if let Ok(cargo_info) = detect_packages(&self.config.cargo_packages, PackageManager::Cargo) {
                host_info.cargo_registry = cargo_info.cargo_registry;
                host_info.cargo_bin = cargo_info.cargo_bin;
                host_info.detected_packages.extend(cargo_info.detected_packages);
            }
        }

        // Get volume mounts and environment variables
        let host_mounts = get_volume_mounts(&host_info);
        let host_env_vars = get_package_env_vars(&host_info);

        // Prepare local pipx package mounts separately (they need rw access)
        let mut local_pipx_mounts = Vec::new();
        if let Some(local_pipx) = self.config.extra_config.get("local_pipx_packages") {
            if let Some(packages) = local_pipx.as_object() {
                for (package_name, source_path) in packages {
                    if let Some(path_str) = source_path.as_str() {
                        let source_path = std::path::PathBuf::from(path_str);
                        if source_path.exists() {
                            // Local packages need read-write access for editable installs
                            let container_path = format!("/opt/local-packages/{}", package_name);
                            local_pipx_mounts.push((
                                path_str.to_string(),
                                container_path,
                            ));
                        }
                    }
                }
            }
        }

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
        context.insert("local_pipx_mounts", &local_pipx_mounts);

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
        // Use shared template engine instead of creating new instance
        let tera = &super::TEMP_COMPOSE_TERA;

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
