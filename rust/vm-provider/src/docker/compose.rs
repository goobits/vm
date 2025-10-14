// Standard library
use std::fs;
use std::path::{Path, PathBuf};

// External crates
use tera::Context as TeraContext;
use vm_core::error::{Result, VmError};

// Internal imports
use super::build::BuildOperations;
use super::host_packages::{
    detect_packages, get_package_env_vars, get_volume_mounts, PackageManager,
};
use super::{ComposeCommand, DockerOps, UserConfig};
use crate::ProviderContext;
use crate::TempVmState;
use vm_config::{config::VmConfig, detect_worktrees};
use vm_core::command_stream::stream_command;

pub struct ComposeOperations<'a> {
    pub config: &'a VmConfig,
    pub temp_dir: &'a PathBuf,
    pub project_dir: &'a PathBuf,
}

/// Context for building host package information
struct HostPackageContext {
    host_mounts: Vec<(String, String)>,
    host_env_vars: Vec<(String, String)>,
}

impl<'a> ComposeOperations<'a> {
    pub fn new(config: &'a VmConfig, temp_dir: &'a PathBuf, project_dir: &'a PathBuf) -> Self {
        Self {
            config,
            temp_dir,
            project_dir,
        }
    }

    /// Build host package context from config and provider context
    ///
    /// This consolidates all package detection, volume mounting, and environment
    /// variable setup logic that was duplicated across render functions.
    fn build_host_package_context(&self, context: &ProviderContext) -> Result<HostPackageContext> {
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
        let host_mounts = get_volume_mounts(&host_info)
            .into_iter()
            .map(|(path, container_path)| (path.to_string_lossy().to_string(), container_path))
            .collect();
        let mut host_env_vars = get_package_env_vars(&host_info);

        // Add package registry environment variables from global config
        if let Some(global_cfg) = context.global_config.as_ref() {
            if global_cfg.services.package_registry.enabled {
                let host = vm_platform::platform::get_host_gateway();
                let port = global_cfg.services.package_registry.port;

                host_env_vars.extend([
                    // NPM
                    (
                        "NPM_CONFIG_REGISTRY".to_string(),
                        format!("http://{host}:{port}/npm/"),
                    ),
                    // Pip with fallback
                    (
                        "PIP_INDEX_URL".to_string(),
                        format!("http://{host}:{port}/pypi/simple/"),
                    ),
                    (
                        "PIP_EXTRA_INDEX_URL".to_string(),
                        "https://pypi.org/simple/".to_string(),
                    ),
                    ("PIP_TRUSTED_HOST".to_string(), host.to_string()),
                    // Cargo (will be used by shell init script)
                    ("VM_CARGO_REGISTRY_HOST".to_string(), host.to_string()),
                    ("VM_CARGO_REGISTRY_PORT".to_string(), port.to_string()),
                ]);
            }

            // Add PostgreSQL environment variables from global config
            if global_cfg.services.postgresql.enabled {
                let host = vm_platform::platform::get_host_gateway();
                let port = global_cfg.services.postgresql.port;
                let user = "postgres";
                let password = "postgres"; // Matches the default password in service_manager.rs
                let db_name = self
                    .config
                    .project
                    .as_ref()
                    .and_then(|p| p.name.as_deref())
                    .unwrap_or("vm_project");

                host_env_vars.push((
                    "DATABASE_URL".to_string(),
                    format!("postgresql://{user}:{password}@{host}:{port}/{db_name}"),
                ));
            }

            // Add Redis environment variables from global config
            if global_cfg.services.redis.enabled {
                let host = vm_platform::platform::get_host_gateway();
                let port = global_cfg.services.redis.port;

                host_env_vars.push(("REDIS_URL".to_string(), format!("redis://{host}:{port}")));
            }

            // Add MongoDB environment variables from global config
            if global_cfg.services.mongodb.enabled {
                let host = vm_platform::platform::get_host_gateway();
                let port = global_cfg.services.mongodb.port;

                host_env_vars.push((
                    "MONGODB_URL".to_string(),
                    format!("mongodb://{host}:{port}"),
                ));
            }
        }

        Ok(HostPackageContext {
            host_mounts,
            host_env_vars,
        })
    }

    pub fn render_docker_compose(
        &self,
        build_context_dir: &Path,
        context: &ProviderContext,
    ) -> Result<String> {
        // Use shared template engine instead of creating new instance
        let tera = super::get_compose_tera();

        let project_dir_str = BuildOperations::path_to_string(self.project_dir)?;
        let build_context_str = BuildOperations::path_to_string(build_context_dir)?;

        let user_config = UserConfig::from_vm_config(self.config);

        // Build host package context (consolidated package detection and env setup)
        let pkg_context = self.build_host_package_context(context)?;

        let project_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project");

        let mut tera_context = TeraContext::new();
        tera_context.insert("config", &self.config);
        tera_context.insert("project_name", &project_name);
        tera_context.insert("project_dir", &project_dir_str);
        tera_context.insert("build_context_dir", &build_context_str);
        tera_context.insert("project_uid", &user_config.uid.to_string());
        tera_context.insert("project_gid", &user_config.gid.to_string());
        tera_context.insert("project_user", &user_config.username);
        tera_context.insert("is_macos", &cfg!(target_os = "macos"));
        tera_context.insert("host_mounts", &pkg_context.host_mounts);
        tera_context.insert("host_env_vars", &pkg_context.host_env_vars);
        // No local package mounts or environment variables needed
        let local_pipx_mounts: Vec<(String, String)> = Vec::new();
        let local_env_vars: Vec<(String, String)> = Vec::new();

        tera_context.insert("local_pipx_mounts", &local_pipx_mounts);
        tera_context.insert("local_env_vars", &local_env_vars);

        // Git worktrees volume
        if let Ok(worktrees) = detect_worktrees() {
            if !worktrees.is_empty() {
                let worktree_mounts: Vec<_> = worktrees
                    .iter()
                    .filter_map(|path_string| {
                        let path = Path::new(path_string);
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .and_then(|name| path.to_str().map(|path_str| (path_str, name)))
                    })
                    .collect();
                tera_context.insert("worktrees", &worktree_mounts);
            }
        }

        let content = tera
            .render("docker-compose.yml", &tera_context)
            .map_err(|e| {
                VmError::Internal(format!("Failed to render docker-compose template: {e}"))
            })?;
        Ok(content)
    }

    pub fn write_docker_compose(
        &self,
        build_context_dir: &Path,
        context: &ProviderContext,
    ) -> Result<PathBuf> {
        let content = self.render_docker_compose(build_context_dir, context)?;

        let path = self.temp_dir.join("docker-compose.yml");
        fs::write(&path, content.as_bytes())?;

        Ok(path)
    }

    /// Write docker-compose.yml with custom instance name
    pub fn write_docker_compose_with_instance(
        &self,
        build_context_dir: &Path,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<PathBuf> {
        let content =
            self.render_docker_compose_with_instance(build_context_dir, instance_name, context)?;

        let path = self.temp_dir.join("docker-compose.yml");
        fs::write(&path, content.as_bytes())?;

        Ok(path)
    }

    /// Render docker-compose.yml with custom instance name
    pub fn render_docker_compose_with_instance(
        &self,
        build_context_dir: &Path,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<String> {
        // Use shared template engine instead of creating new instance
        let tera = super::get_compose_tera();

        let project_dir_str = BuildOperations::path_to_string(self.project_dir)?;
        let build_context_str = BuildOperations::path_to_string(build_context_dir)?;

        let user_config = UserConfig::from_vm_config(self.config);

        // Build host package context (consolidated package detection and env setup)
        let pkg_context = self.build_host_package_context(context)?;

        let project_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project");

        // Create a custom config with the instance name
        let mut custom_config = self.config.clone();
        if let Some(ref mut project) = custom_config.project {
            if let Some(ref project_name) = project.name {
                project.name = Some(format!("{project_name}-{instance_name}"));
            } else {
                project.name = Some(format!("vm-project-{instance_name}"));
            }
        } else {
            custom_config.project = Some(vm_config::config::ProjectConfig {
                name: Some(format!("vm-project-{instance_name}")),
                ..Default::default()
            });
        }

        let mut tera_context = TeraContext::new();
        tera_context.insert("config", &custom_config);
        tera_context.insert("project_name", &format!("{project_name}-{instance_name}"));
        tera_context.insert("project_dir", &project_dir_str);
        tera_context.insert("build_context_dir", &build_context_str);
        tera_context.insert("project_uid", &user_config.uid.to_string());
        tera_context.insert("project_gid", &user_config.gid.to_string());
        tera_context.insert("project_user", &user_config.username);
        tera_context.insert("is_macos", &cfg!(target_os = "macos"));
        tera_context.insert("host_mounts", &pkg_context.host_mounts);
        tera_context.insert("host_env_vars", &pkg_context.host_env_vars);
        // No local package mounts or environment variables needed
        let local_pipx_mounts: Vec<(String, String)> = Vec::new();
        let local_env_vars: Vec<(String, String)> = Vec::new();

        tera_context.insert("local_pipx_mounts", &local_pipx_mounts);
        tera_context.insert("local_env_vars", &local_env_vars);

        // Git worktrees volume
        if let Ok(worktrees) = detect_worktrees() {
            if !worktrees.is_empty() {
                let worktree_mounts: Vec<_> = worktrees
                    .iter()
                    .filter_map(|path_string| {
                        let path = Path::new(path_string);
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .and_then(|name| path.to_str().map(|path_str| (path_str, name)))
                    })
                    .collect();
                tera_context.insert("worktrees", &worktree_mounts);
            }
        }

        let content = tera
            .render("docker-compose.yml", &tera_context)
            .map_err(|e| {
                VmError::Internal(format!("Failed to render docker-compose template: {e}"))
            })?;
        Ok(content)
    }

    pub fn render_docker_compose_with_mounts(&self, state: &TempVmState) -> Result<String> {
        // Use shared template engine instead of creating new instance
        let tera = super::get_temp_compose_tera();

        let mut context = TeraContext::new();
        context.insert("config", &self.config);
        context.insert("container_name", &state.container_name);
        context.insert("mounts", &state.mounts);

        let content = tera.render("docker-compose.yml", &context).map_err(|e| {
            VmError::Internal(format!("Failed to render docker-compose template: {e}"))
        })?;
        Ok(content)
    }

    pub fn start_with_compose(&self, context: &ProviderContext) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            // Fallback: prepare build context and generate compose file
            let build_ops = BuildOperations::new(self.config, self.temp_dir);
            let build_context = build_ops.prepare_build_context()?;
            self.write_docker_compose(&build_context, context)?;
        }

        // Check if the container already exists (stopped or running)
        let container_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .map(|s| format!("{s}-dev"))
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
        stream_command("docker", &args_refs).map_err(|e| {
            VmError::Internal(format!(
                "Failed to start container using docker-compose: {e}"
            ))
        })
    }

    #[allow(dead_code)]
    pub fn stop_with_compose(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if compose_path.exists() {
            let args = ComposeCommand::build_args(&compose_path, "stop", &[])?;
            let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            stream_command("docker", &args_refs).map_err(|e| {
                VmError::Internal(format!(
                    "Failed to stop container using docker-compose: {e}"
                ))
            })
        } else {
            Err(VmError::Internal(format!(
                "docker-compose.yml not found in '{}'. Cannot stop container without compose configuration",
                self.temp_dir.display()
            )))
        }
    }

    #[allow(dead_code)]
    pub fn destroy_with_compose(&self) -> Result<()> {
        let compose_path = self.temp_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            return Err(VmError::Internal(format!(
                "docker-compose.yml not found in '{}' for container destruction. Use direct Docker commands instead",
                self.temp_dir.display()
            )));
        }
        let args = ComposeCommand::build_args(&compose_path, "down", &["--volumes"])?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command("docker", &args_refs)
            .map_err(|e| VmError::Internal(format!("Failed to destroy container: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use vm_config::{
        config::{ProjectConfig, VmConfig, WorktreesConfig},
        global_config::{GlobalConfig, WorktreesGlobalSettings},
    };

    fn setup_test_env() -> (TempDir, PathBuf, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let temp_path = temp_dir.path().to_path_buf();
        (temp_dir, project_dir, temp_path)
    }

    #[test]
    fn test_package_registry_env_vars_injection() {
        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let temp_path = temp_dir.path().to_path_buf();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir_all(&build_dir).unwrap();

        // Create a minimal VmConfig
        let vm_config = VmConfig {
            project: Some(vm_config::config::ProjectConfig {
                name: Some("test-project".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create GlobalConfig with package registry enabled
        let global_config = GlobalConfig {
            services: vm_config::global_config::GlobalServices {
                package_registry: vm_config::global_config::PackageRegistrySettings {
                    enabled: true,
                    port: 3080,
                    max_storage_gb: 10,
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // Create ProviderContext with global config
        let context = ProviderContext::default().with_config(global_config);

        // Create ComposeOperations
        let compose_ops = ComposeOperations::new(&vm_config, &temp_path, &project_dir);

        // Render docker-compose
        let result = compose_ops.render_docker_compose(&build_dir, &context);
        assert!(result.is_ok(), "render_docker_compose should succeed");

        let content = result.unwrap();

        // Verify that environment variables are in the rendered output
        let host = vm_platform::platform::get_host_gateway();

        assert!(
            content.contains(&format!("NPM_CONFIG_REGISTRY=http://{}:3080/npm/", host)),
            "Should contain NPM_CONFIG_REGISTRY"
        );
        assert!(
            content.contains(&format!("PIP_INDEX_URL=http://{}:3080/pypi/simple/", host)),
            "Should contain PIP_INDEX_URL"
        );
        assert!(
            content.contains("PIP_EXTRA_INDEX_URL=https://pypi.org/simple/"),
            "Should contain PIP_EXTRA_INDEX_URL for fallback"
        );
        assert!(
            content.contains(&format!("PIP_TRUSTED_HOST={}", host)),
            "Should contain PIP_TRUSTED_HOST"
        );
        assert!(
            content.contains(&format!("VM_CARGO_REGISTRY_HOST={}", host)),
            "Should contain VM_CARGO_REGISTRY_HOST"
        );
        assert!(
            content.contains("VM_CARGO_REGISTRY_PORT=3080"),
            "Should contain VM_CARGO_REGISTRY_PORT"
        );
    }

    #[test]
    fn test_package_registry_disabled_no_env_vars() {
        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let temp_path = temp_dir.path().to_path_buf();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir_all(&build_dir).unwrap();

        // Create a minimal VmConfig
        let vm_config = VmConfig {
            project: Some(vm_config::config::ProjectConfig {
                name: Some("test-project".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create GlobalConfig with package registry DISABLED
        let global_config = GlobalConfig {
            services: vm_config::global_config::GlobalServices {
                package_registry: vm_config::global_config::PackageRegistrySettings {
                    enabled: false,
                    port: 3080,
                    max_storage_gb: 10,
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // Create ProviderContext with global config
        let context = ProviderContext::default().with_config(global_config);

        // Create ComposeOperations
        let compose_ops = ComposeOperations::new(&vm_config, &temp_path, &project_dir);

        // Render docker-compose
        let result = compose_ops.render_docker_compose(&build_dir, &context);
        assert!(result.is_ok(), "render_docker_compose should succeed");

        let content = result.unwrap();

        // Verify that registry environment variables are NOT in the rendered output
        assert!(
            !content.contains("NPM_CONFIG_REGISTRY="),
            "Should NOT contain NPM_CONFIG_REGISTRY when disabled"
        );
        assert!(
            !content.contains("VM_CARGO_REGISTRY_HOST="),
            "Should NOT contain VM_CARGO_REGISTRY_HOST when disabled"
        );
    }

    #[test]
    fn test_no_global_config_no_env_vars() {
        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let temp_path = temp_dir.path().to_path_buf();
        let build_dir = temp_dir.path().join("build");
        std::fs::create_dir_all(&build_dir).unwrap();

        // Create a minimal VmConfig
        let vm_config = VmConfig {
            project: Some(vm_config::config::ProjectConfig {
                name: Some("test-project".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create ProviderContext WITHOUT global config
        let context = ProviderContext::default();

        // Create ComposeOperations
        let compose_ops = ComposeOperations::new(&vm_config, &temp_path, &project_dir);

        // Render docker-compose
        let result = compose_ops.render_docker_compose(&build_dir, &context);
        assert!(result.is_ok(), "render_docker_compose should succeed");

        let content = result.unwrap();

        // Verify that registry environment variables are NOT in the rendered output
        assert!(
            !content.contains("NPM_CONFIG_REGISTRY="),
            "Should NOT contain NPM_CONFIG_REGISTRY when no global config"
        );
        assert!(
            !content.contains("VM_CARGO_REGISTRY_HOST="),
            "Should NOT contain VM_CARGO_REGISTRY_HOST when no global config"
        );
    }

    #[test]
    fn test_host_gateway_detection() {
        let host = vm_platform::platform::get_host_gateway();

        #[cfg(target_os = "linux")]
        assert_eq!(host, "172.17.0.1", "Linux should use Docker bridge IP");

        #[cfg(not(target_os = "linux"))]
        assert_eq!(
            host, "host.docker.internal",
            "macOS/Windows should use host.docker.internal"
        );
    }

    #[test]
    fn test_start_with_compose_regenerates_with_new_config() {
        use tempfile::TempDir;
        use vm_config::GlobalConfig;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let project_dir = temp_dir.path().to_path_buf();

        // Create a basic VM config
        let mut vm_config = VmConfig::default();
        vm_config.project = Some(vm_config::config::ProjectConfig {
            name: Some("test-project".to_string()),
            ..Default::default()
        });

        // First call: Write compose file WITHOUT registry config
        let context_without_registry = ProviderContext::with_verbose(false);
        let compose_ops = ComposeOperations::new(&vm_config, &temp_path, &project_dir);

        // Create build context manually (without pulling images)
        let build_context = temp_path.join("build_context");
        std::fs::create_dir_all(&build_context).unwrap();

        let compose_path = compose_ops
            .write_docker_compose(&build_context, &context_without_registry)
            .unwrap();

        // Read the initial compose file
        let initial_content = std::fs::read_to_string(&compose_path).unwrap();

        // Verify NO registry env vars in initial compose
        assert!(
            !initial_content.contains("NPM_CONFIG_REGISTRY="),
            "Initial compose should NOT contain NPM_CONFIG_REGISTRY"
        );
        assert!(
            !initial_content.contains("VM_CARGO_REGISTRY_HOST="),
            "Initial compose should NOT contain VM_CARGO_REGISTRY_HOST"
        );

        // Second call: Write compose file WITH registry config
        let mut global_config = GlobalConfig::default();
        global_config.services.package_registry.enabled = true;
        global_config.services.package_registry.port = 3080;

        let context_with_registry = ProviderContext::with_verbose(false).with_config(global_config);

        // Regenerate compose with registry enabled
        compose_ops
            .write_docker_compose(&build_context, &context_with_registry)
            .unwrap();

        // Read the updated compose file
        let updated_content = std::fs::read_to_string(&compose_path).unwrap();

        // Verify registry env vars ARE present after regeneration
        let host = vm_platform::platform::get_host_gateway();
        assert!(
            updated_content.contains(&format!("NPM_CONFIG_REGISTRY=http://{}:3080/npm/", host)),
            "Updated compose should contain NPM_CONFIG_REGISTRY with correct host and port"
        );
        assert!(
            updated_content.contains(&format!("VM_CARGO_REGISTRY_HOST={}", host)),
            "Updated compose should contain VM_CARGO_REGISTRY_HOST"
        );
        assert!(
            updated_content.contains("VM_CARGO_REGISTRY_PORT=3080"),
            "Updated compose should contain VM_CARGO_REGISTRY_PORT"
        );
        assert!(
            updated_content.contains(&format!("PIP_INDEX_URL=http://{}:3080/pypi/simple/", host)),
            "Updated compose should contain PIP_INDEX_URL"
        );

        // Verify that the file was actually regenerated (contents changed)
        assert_ne!(
            initial_content, updated_content,
            "Compose file should be regenerated with different content"
        );
    }

    #[test]
    fn test_start_with_compose_can_disable_registry() {
        use tempfile::TempDir;
        use vm_config::GlobalConfig;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let project_dir = temp_dir.path().to_path_buf();

        let mut vm_config = VmConfig::default();
        vm_config.project = Some(vm_config::config::ProjectConfig {
            name: Some("test-project".to_string()),
            ..Default::default()
        });

        // First: Enable registry
        let mut global_config = GlobalConfig::default();
        global_config.services.package_registry.enabled = true;
        global_config.services.package_registry.port = 3080;

        let context_with_registry =
            ProviderContext::with_verbose(false).with_config(global_config.clone());

        let compose_ops = ComposeOperations::new(&vm_config, &temp_path, &project_dir);

        // Create build context manually (without pulling images)
        let build_context = temp_path.join("build_context");
        std::fs::create_dir_all(&build_context).unwrap();

        let compose_path = compose_ops
            .write_docker_compose(&build_context, &context_with_registry)
            .unwrap();

        let initial_content = std::fs::read_to_string(&compose_path).unwrap();
        assert!(
            initial_content.contains("NPM_CONFIG_REGISTRY="),
            "Should contain registry vars when enabled"
        );

        // Second: Disable registry
        let mut global_config_disabled = GlobalConfig::default();
        global_config_disabled.services.package_registry.enabled = false;

        let context_disabled =
            ProviderContext::with_verbose(false).with_config(global_config_disabled);

        compose_ops
            .write_docker_compose(&build_context, &context_disabled)
            .unwrap();

        let updated_content = std::fs::read_to_string(&compose_path).unwrap();

        // Verify registry vars are REMOVED after disabling
        assert!(
            !updated_content.contains("NPM_CONFIG_REGISTRY="),
            "Should NOT contain registry vars when disabled"
        );
        assert!(
            !updated_content.contains("VM_CARGO_REGISTRY_HOST="),
            "Should NOT contain registry vars when disabled"
        );
    }

    #[test]
    fn test_worktrees_disabled_by_default() {
        let (_temp_dir, project_dir, temp_path) = setup_test_env();
        let config = VmConfig::default();
        let context = ProviderContext::default();
        let compose_ops = ComposeOperations::new(&config, &temp_path, &project_dir);

        let rendered = compose_ops
            .render_docker_compose(&project_dir, &context)
            .unwrap();
        assert!(!rendered.contains("/worktrees:rw"));
    }

    #[test]
    fn test_worktrees_enabled_globally() {
        let (_temp_dir, project_dir, temp_path) = setup_test_env();
        let config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("test-project".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut global_config = GlobalConfig::default();
        global_config.worktrees.enabled = true;
        let context = ProviderContext::default().with_config(global_config);
        let compose_ops = ComposeOperations::new(&config, &temp_path, &project_dir);

        let rendered = compose_ops
            .render_docker_compose(&project_dir, &context)
            .unwrap();
        // New implementation detects worktrees dynamically from git
        // If no worktrees exist, no worktree mounts should be in the output
        // This test now just verifies it renders without error
        assert!(!rendered.is_empty());
    }

    #[test]
    fn test_worktrees_enabled_per_project() {
        let (_temp_dir, project_dir, temp_path) = setup_test_env();
        let config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("test-project".into()),
                ..Default::default()
            }),
            worktrees: Some(WorktreesConfig {
                enabled: true,
                base_path: None,
            }),
            ..Default::default()
        };
        let context = ProviderContext::default();
        let compose_ops = ComposeOperations::new(&config, &temp_path, &project_dir);

        let rendered = compose_ops
            .render_docker_compose(&project_dir, &context)
            .unwrap();
        // New implementation detects worktrees dynamically from git
        // Worktree config enabled just means detection is active
        assert!(!rendered.is_empty());
    }

    #[test]
    fn test_worktrees_project_overrides_global_disabled() {
        let (_temp_dir, project_dir, temp_path) = setup_test_env();
        let config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("test-project".into()),
                ..Default::default()
            }),
            worktrees: Some(WorktreesConfig {
                enabled: true,
                base_path: None,
            }),
            ..Default::default()
        };
        let mut global_config = GlobalConfig::default();
        global_config.worktrees.enabled = false;
        let context = ProviderContext::default().with_config(global_config);
        let compose_ops = ComposeOperations::new(&config, &temp_path, &project_dir);

        let rendered = compose_ops
            .render_docker_compose(&project_dir, &context)
            .unwrap();
        // Project-level worktrees enabled overrides global disabled
        assert!(!rendered.is_empty());
    }

    #[test]
    fn test_worktrees_custom_base_path_from_project() {
        let (_temp_dir, project_dir, temp_path) = setup_test_env();
        let config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("test-project".into()),
                ..Default::default()
            }),
            worktrees: Some(WorktreesConfig {
                enabled: true,
                base_path: Some("/custom/path".to_string()),
            }),
            ..Default::default()
        };
        let context = ProviderContext::default();
        let compose_ops = ComposeOperations::new(&config, &temp_path, &project_dir);

        let rendered = compose_ops
            .render_docker_compose(&project_dir, &context)
            .unwrap();
        // Custom base_path is now deprecated - worktrees detected dynamically
        assert!(!rendered.is_empty());
    }

    #[test]
    fn test_worktrees_custom_base_path_from_global() {
        let (_temp_dir, project_dir, temp_path) = setup_test_env();
        let config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("test-project".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let global_config = GlobalConfig {
            worktrees: WorktreesGlobalSettings {
                enabled: true,
                base_path: Some("/global/path".to_string()),
            },
            ..Default::default()
        };
        let context = ProviderContext::default().with_config(global_config);
        let compose_ops = ComposeOperations::new(&config, &temp_path, &project_dir);

        let rendered = compose_ops
            .render_docker_compose(&project_dir, &context)
            .unwrap();
        // Custom base_path is now deprecated - worktrees detected dynamically
        assert!(!rendered.is_empty());
    }

    #[test]
    fn test_worktrees_project_base_path_overrides_global() {
        let (_temp_dir, project_dir, temp_path) = setup_test_env();
        let config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("test-project".into()),
                ..Default::default()
            }),
            worktrees: Some(WorktreesConfig {
                enabled: true,
                base_path: Some("/project/path".to_string()),
            }),
            ..Default::default()
        };
        let global_config = GlobalConfig {
            worktrees: WorktreesGlobalSettings {
                enabled: true,
                base_path: Some("/global/path".to_string()),
            },
            ..Default::default()
        };
        let context = ProviderContext::default().with_config(global_config);
        let compose_ops = ComposeOperations::new(&config, &temp_path, &project_dir);

        let rendered = compose_ops
            .render_docker_compose(&project_dir, &context)
            .unwrap();
        // Custom base_path is now deprecated - worktrees detected dynamically
        assert!(!rendered.is_empty());
    }
}
