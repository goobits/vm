use std::fs;
use std::path::{Path, PathBuf};

// External crates
use tera::Context as TeraContext;
use vm_core::error::{Result, VmError};

// Internal imports
use super::artifacts::{compose_path, secure_write_if_changed};
use super::build::BuildOperations;
use super::compose_context::{
    build_host_package_context, configure_ssh_agent, configure_worktrees, ensure_ai_sync_dirs,
    process_dotfiles,
};
use super::compose_model::{RenderedResources, RenderedStorage};
use super::preview::redact_compose;
use super::{ComposeCommand, DockerOps, UserConfig};
use crate::user_home::resolve_home_dir;
use crate::ProviderContext;
use crate::TempVmState;
use vm_config::config::VmConfig;
use vm_core::command_stream::stream_command_visible;

pub struct ComposeOperations<'a> {
    pub config: &'a VmConfig,
    pub generated_dir: &'a Path,
    pub project_dir: &'a Path,
    pub executable: &'a str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RenderMode {
    Runtime,
    Preview,
}

impl<'a> ComposeOperations<'a> {
    pub fn new(
        config: &'a VmConfig,
        generated_dir: &'a Path,
        project_dir: &'a Path,
        executable: &'a str,
    ) -> Self {
        Self {
            config,
            generated_dir,
            project_dir,
            executable,
        }
    }

    /// Helper to create config with instance name suffix
    fn create_instance_config(
        &self,
        base_project_name: &str,
        instance: &str,
    ) -> (VmConfig, String) {
        let mut custom_config = self.config.clone();

        // Determine instance project name
        let instance_project_name = custom_config
            .project
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .map(|name| format!("{}-{}", name, instance))
            .unwrap_or_else(|| format!("vm-project-{}", instance));

        // Update or create project config
        let project = custom_config.project.get_or_insert_with(Default::default);
        project.name = Some(instance_project_name.clone());

        let final_name = format!("{}-{}", base_project_name, instance);
        (custom_config, final_name)
    }

    /// Internal method that handles rendering with optional instance name
    fn render_docker_compose_internal(
        &self,
        build_context_dir: &Path,
        instance_name: Option<&str>,
        context: &ProviderContext,
        image_tag: Option<&str>,
        mode: RenderMode,
    ) -> Result<String> {
        // Use shared template engine instead of creating new instance
        let tera = super::get_compose_tera();

        let project_dir_str = BuildOperations::path_to_string(self.project_dir)?;
        let build_context_str = BuildOperations::path_to_string(build_context_dir)?;

        let user_config = UserConfig::from_vm_config(self.config);

        // Build host package context (consolidated package detection and env setup)
        let mut pkg_context = build_host_package_context(self.config, context);
        if mode == RenderMode::Preview {
            for (_, value) in &mut pkg_context.host_env_vars {
                *value = "<redacted>".to_string();
            }
        }

        let base_project_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project");

        // Handle instance name modification if provided
        let (mut final_config, final_project_name) = match instance_name {
            Some(instance) => self.create_instance_config(base_project_name, instance),
            None => (self.config.clone(), base_project_name.to_string()),
        };
        if mode == RenderMode::Preview {
            for value in final_config.environment.values_mut() {
                *value = "<redacted>".to_string();
            }
        }

        let storage = RenderedStorage::new(&final_config, base_project_name, &final_project_name);
        let resources = RenderedResources::resolve(&final_config)?;

        let mut tera_context = TeraContext::new();
        tera_context.insert("config", &final_config);
        tera_context.insert("project_name", &final_project_name);
        tera_context.insert("base_project_name", &base_project_name);
        tera_context.insert("storage_volumes", &storage.mounts);
        tera_context.insert("named_volumes", &storage.named_volumes);
        tera_context.insert("tmpfs_mounts", &storage.tmpfs);
        tera_context.insert("resources", &resources);
        tera_context.insert("project_dir", &project_dir_str);
        tera_context.insert("build_context_dir", &build_context_str);
        tera_context.insert("project_uid", &user_config.uid.to_string());
        tera_context.insert("project_gid", &user_config.gid.to_string());
        tera_context.insert("project_user", &user_config.username);
        tera_context.insert(
            "build_user_args_enabled",
            &(!BuildOperations::new(self.config, self.generated_dir, self.executable)
                .uses_preprovisioned_snapshot()),
        );
        tera_context.insert(
            "image_tag",
            &image_tag
                .map(std::string::ToString::to_string)
                .unwrap_or_else(|| format!("{final_project_name}:latest")),
        );
        tera_context.insert("is_macos", &cfg!(target_os = "macos"));
        tera_context.insert("host_mounts", &pkg_context.host_mounts);
        tera_context.insert("host_env_vars", &pkg_context.host_env_vars);

        // AI sync flags for template
        if let Some(ai_sync) = &self
            .config
            .host_sync
            .as_ref()
            .and_then(|hs| hs.ai_tools.as_ref())
        {
            tera_context.insert("claude_sync_enabled", &ai_sync.is_claude_enabled());
            tera_context.insert("gemini_sync_enabled", &ai_sync.is_gemini_enabled());
            tera_context.insert("codex_sync_enabled", &ai_sync.is_codex_enabled());
        }
        // No local package mounts or environment variables needed
        let local_pipx_mounts: Vec<(String, String)> = Vec::new();
        let local_env_vars: Vec<(String, String)> = Vec::new();

        tera_context.insert("local_pipx_mounts", &local_pipx_mounts);
        tera_context.insert("local_env_vars", &local_env_vars);

        // SSH agent forwarding
        configure_ssh_agent(self.config, &mut tera_context);

        // Dotfiles sync
        let dotfile_mounts = process_dotfiles(self.config, &user_config.username);
        if !dotfile_mounts.is_empty() {
            tera_context.insert("dotfile_mounts", &dotfile_mounts);
        }

        // Get home directory for template (needed for AI tools sync)
        let home_dir = resolve_home_dir()
            .map(|home| home.to_string_lossy().to_string())
            .unwrap_or_else(|| "/home/developer".to_string());
        tera_context.insert("home_dir", &home_dir);

        // Git worktrees volume
        configure_worktrees(
            self.config,
            &mut tera_context,
            &home_dir,
            &final_project_name,
            mode == RenderMode::Runtime,
        );

        // Get or generate passwords for database services
        // Note: Using sync version since we're in a non-async context
        if final_config
            .services
            .get("postgresql")
            .is_some_and(|s| s.enabled)
        {
            if mode == RenderMode::Preview {
                tera_context.insert("postgresql_password", "<redacted>");
            } else {
                match vm_core::secrets::get_or_generate_password_sync("postgresql") {
                    Ok(password) => {
                        tera_context.insert("postgresql_password", &password);
                    }
                    Err(e) => {
                        return Err(VmError::Internal(format!(
                            "Failed to load or create the PostgreSQL password: {e}"
                        )));
                    }
                }
            }
        }

        let content = tera
            .render("docker-compose.yml", &tera_context)
            .map_err(|e| {
                eprintln!("Tera render error: {:?}", e);
                VmError::Internal(format!("Failed to render docker-compose template: {:?}", e))
            })?;
        Ok(content)
    }

    /// Render docker-compose.yml without instance name
    pub fn render_docker_compose(
        &self,
        build_context_dir: &Path,
        context: &ProviderContext,
    ) -> Result<String> {
        self.render_docker_compose_internal(
            build_context_dir,
            None,
            context,
            None,
            RenderMode::Runtime,
        )
    }

    pub fn write_docker_compose(
        &self,
        build_context_dir: &Path,
        context: &ProviderContext,
    ) -> Result<PathBuf> {
        // Ensure AI sync directories exist before rendering compose file
        ensure_ai_sync_dirs(self.config)?;

        let content = self.render_docker_compose(build_context_dir, context)?;

        let path = compose_path(self.generated_dir, None);
        secure_write_if_changed(&path, content.as_bytes())?;

        Ok(path)
    }

    /// Write docker-compose.yml with custom instance name
    pub fn write_docker_compose_with_instance(
        &self,
        build_context_dir: &Path,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<PathBuf> {
        // Ensure AI sync directories exist before rendering compose file
        ensure_ai_sync_dirs(self.config)?;

        let content =
            self.render_docker_compose_with_instance(build_context_dir, instance_name, context)?;

        let path = compose_path(self.generated_dir, Some(instance_name));
        secure_write_if_changed(&path, content.as_bytes())?;

        Ok(path)
    }

    /// Render docker-compose.yml with custom instance name
    pub fn render_docker_compose_with_instance(
        &self,
        build_context_dir: &Path,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<String> {
        self.render_docker_compose_internal(
            build_context_dir,
            Some(instance_name),
            context,
            None,
            RenderMode::Runtime,
        )
    }

    pub fn write_docker_compose_with_image_tag(
        &self,
        build_context_dir: &Path,
        context: &ProviderContext,
        image_tag: &str,
    ) -> Result<PathBuf> {
        ensure_ai_sync_dirs(self.config)?;

        let content = self.render_docker_compose_internal(
            build_context_dir,
            None,
            context,
            Some(image_tag),
            RenderMode::Runtime,
        )?;

        let path = compose_path(self.generated_dir, None);
        secure_write_if_changed(&path, content.as_bytes())?;

        Ok(path)
    }

    pub fn write_docker_compose_with_instance_and_image_tag(
        &self,
        build_context_dir: &Path,
        instance_name: &str,
        context: &ProviderContext,
        image_tag: &str,
    ) -> Result<PathBuf> {
        ensure_ai_sync_dirs(self.config)?;

        let content = self.render_docker_compose_internal(
            build_context_dir,
            Some(instance_name),
            context,
            Some(image_tag),
            RenderMode::Runtime,
        )?;

        let path = compose_path(self.generated_dir, Some(instance_name));
        secure_write_if_changed(&path, content.as_bytes())?;

        Ok(path)
    }

    pub fn render_docker_compose_preview(
        &self,
        build_context_dir: &Path,
        instance_name: Option<&str>,
        context: &ProviderContext,
    ) -> Result<String> {
        let content = self.render_docker_compose_internal(
            build_context_dir,
            instance_name,
            context,
            None,
            RenderMode::Preview,
        )?;
        redact_compose(&content)
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
        let container_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .map(|s| format!("{s}-dev"))
            .unwrap_or_else(|| "vm-project-dev".to_string());
        self.start_named_with_compose(&container_name, context)
    }

    pub fn start_named_with_compose(
        &self,
        container_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        let instance_name = self.instance_name_from_container(container_name);
        let compose_path = compose_path(self.generated_dir, instance_name.as_deref());
        let container_exists =
            DockerOps::container_exists(Some(self.executable), container_name).unwrap_or(false);

        if !container_exists || !compose_path.exists() {
            let build_ops = BuildOperations::new(self.config, self.generated_dir, self.executable);
            let build_context = build_ops.prepare_compose_build_context()?;
            if let Some(instance_name) = &instance_name {
                self.write_docker_compose_with_instance(&build_context, instance_name, context)?;
            } else {
                self.write_docker_compose(&build_context, context)?;
            }
        }

        // If the dev container exists, start it directly to avoid compose name conflicts
        // with preserved service containers (e.g., postgres).
        if container_exists {
            // Start any existing service containers (if stopped).
            let expected_services = self.get_expected_service_containers(instance_name.as_deref());
            for service in expected_services {
                if !DockerOps::container_exists(Some(self.executable), &service).unwrap_or(false) {
                    continue;
                }
                let running = DockerOps::is_container_running(Some(self.executable), &service)
                    .unwrap_or(false);
                if running {
                    continue;
                }
                DockerOps::start_container(Some(self.executable), &service)?;
            }

            // Start the dev container if it's not already running.
            let dev_running =
                DockerOps::is_container_running(Some(self.executable), container_name)
                    .unwrap_or(false);
            if !dev_running {
                DockerOps::start_container(Some(self.executable), container_name)?;
            }
            return Ok(());
        }

        // No existing dev container. Fall back to compose up to create/start everything.
        let (command, extra_args): (&str, Vec<String>) = ("up", vec!["-d".to_string()]);
        let extra_args_refs: Vec<&str> = extra_args.iter().map(|s| s.as_str()).collect();
        let args = ComposeCommand::build_args(&compose_path, command, &extra_args_refs)?;
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        stream_command_visible(self.executable, &args_refs).map_err(|e| {
            VmError::Internal(format!(
                "Failed to start container using docker-compose: {e}"
            ))
        })
    }

    pub(super) fn instance_name_from_container(&self, container_name: &str) -> Option<String> {
        let project_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project");
        container_name
            .strip_prefix(&format!("{project_name}-"))
            .and_then(|name| name.strip_suffix("-dev"))
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    }

    /// Get list of expected service container names by parsing the generated docker-compose.yml.
    ///
    /// Returns a list of container names that are expected to be created by docker-compose
    /// for the enabled services. Used for orphan detection.
    pub fn get_expected_service_containers(&self, instance_name: Option<&str>) -> Vec<String> {
        let compose_path = compose_path(self.generated_dir, instance_name);
        if !compose_path.exists() {
            return Vec::new();
        }

        let Ok(content) = fs::read_to_string(&compose_path) else {
            return Vec::new();
        };

        let Ok(yaml) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&content) else {
            return Vec::new();
        };

        let Some(services) = yaml.get("services").and_then(|v| v.as_mapping()) else {
            return Vec::new();
        };

        let project_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project");

        let mut expected = Vec::new();

        for (service_name, service_config) in services {
            let Some(service_name_str) = service_name.as_str() else {
                continue;
            };

            // Skip the main dev container
            if service_name_str.ends_with("-dev") {
                continue;
            }

            // Check for explicit container_name
            if let Some(container_name) = service_config
                .get("container_name")
                .and_then(|v| v.as_str())
            {
                expected.push(container_name.to_string());
            } else {
                // Use Compose default: {project}_{service}_1
                expected.push(format!("{}_{}_1", project_name, service_name_str));
            }
        }

        expected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::compose_model::container_architecture;
    use tempfile::TempDir;
    use vm_config::{
        config::{
            ContainerLoggingConfig, CpuLimit, MemoryLimit, ProjectConfig, StorageConfig,
            TmpfsMountConfig, VmConfig, VmSettings, VolumeMountConfig, VolumeRetention,
            VolumeScope,
        },
        global_config::GlobalConfig,
    };

    fn setup_test_env() -> (TempDir, PathBuf, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let temp_path = temp_dir.path().to_path_buf();
        (temp_dir, project_dir, temp_path)
    }

    fn yaml_mapping<'a>(value: &'a serde_yaml_ng::Value, key: &str) -> &'a serde_yaml_ng::Mapping {
        value
            .get(key)
            .and_then(serde_yaml_ng::Value::as_mapping)
            .unwrap_or_else(|| panic!("missing YAML mapping: {key}"))
    }

    fn volume_mount<'a>(
        service: &'a serde_yaml_ng::Mapping,
        source: &str,
    ) -> &'a serde_yaml_ng::Mapping {
        service
            .get("volumes")
            .and_then(serde_yaml_ng::Value::as_sequence)
            .and_then(|mounts| {
                mounts.iter().find_map(|mount| {
                    let mapping = mount.as_mapping()?;
                    (mapping.get("source").and_then(serde_yaml_ng::Value::as_str) == Some(source))
                        .then_some(mapping)
                })
            })
            .unwrap_or_else(|| panic!("missing volume mount: {source}"))
    }

    #[test]
    fn renders_stable_scoped_storage_and_runtime_policy() {
        let (_temp_dir, project_dir, temp_path) = setup_test_env();
        let mut volumes = indexmap::IndexMap::new();
        volumes.insert(
            "node_modules".to_string(),
            VolumeMountConfig {
                target: "/workspace/node_modules".to_string(),
                scope: VolumeScope::Instance,
                nocopy: true,
                retention: VolumeRetention::Keep,
            },
        );
        volumes.insert(
            "pnpm_store".to_string(),
            VolumeMountConfig {
                target: "/home/developer/.local/share/pnpm/store".to_string(),
                scope: VolumeScope::Platform,
                nocopy: true,
                retention: VolumeRetention::Keep,
            },
        );
        volumes.insert(
            "scratch".to_string(),
            VolumeMountConfig {
                target: "/var/cache/project".to_string(),
                scope: VolumeScope::Project,
                nocopy: true,
                retention: VolumeRetention::Disposable,
            },
        );
        let config = VmConfig {
            provider: Some("docker".to_string()),
            project: Some(ProjectConfig {
                name: Some("sketch-api".to_string()),
                ..Default::default()
            }),
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Limited(20_480)),
                cpus: Some(CpuLimit::Unlimited),
                pids_limit: Some(4096),
                stop_grace_period: Some(60),
                logging: Some(ContainerLoggingConfig::default()),
                ..Default::default()
            }),
            storage: StorageConfig {
                volumes,
                tmpfs: vec![TmpfsMountConfig {
                    target: "/tmp".to_string(),
                    size: MemoryLimit::Limited(4096),
                    mode: "1777".to_string(),
                }],
            },
            ..Default::default()
        };
        let compose = ComposeOperations::new(&config, &temp_path, &project_dir, "docker");
        let context = ProviderContext::default();

        let rendered = compose
            .render_docker_compose(&project_dir, &context)
            .unwrap();
        assert_eq!(
            rendered,
            compose
                .render_docker_compose(&project_dir, &context)
                .unwrap(),
            "rendering the same project must be deterministic"
        );

        let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).unwrap();
        let services = yaml_mapping(&yaml, "services");
        let dev = services
            .get("sketch-api-dev")
            .and_then(serde_yaml_ng::Value::as_mapping)
            .unwrap();

        assert_eq!(
            dev.get("mem_limit").and_then(|value| value.as_str()),
            Some("20480m")
        );
        assert!(
            !dev.contains_key("cpus"),
            "unlimited CPUs must omit the limit"
        );
        assert_eq!(
            dev.get("pids_limit").and_then(|value| value.as_u64()),
            Some(4096)
        );
        assert_eq!(
            dev.get("stop_grace_period")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("60s")
        );
        assert_eq!(
            dev.get("restart").and_then(|value| value.as_str()),
            Some("no")
        );
        let labels = dev
            .get("labels")
            .and_then(serde_yaml_ng::Value::as_mapping)
            .unwrap();
        assert_eq!(
            labels
                .get("com.vm.project")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("sketch-api")
        );
        assert_eq!(
            labels
                .get("com.vm.instance")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("sketch-api")
        );

        let logging = dev
            .get("logging")
            .and_then(serde_yaml_ng::Value::as_mapping)
            .unwrap();
        assert_eq!(
            logging.get("driver").and_then(|value| value.as_str()),
            Some("local")
        );
        let logging_options = logging
            .get("options")
            .and_then(serde_yaml_ng::Value::as_mapping)
            .unwrap();
        assert_eq!(
            logging_options
                .get("max-size")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("20m")
        );
        assert_eq!(
            logging_options
                .get("max-file")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("5")
        );

        assert_eq!(
            dev.get("tmpfs")
                .and_then(serde_yaml_ng::Value::as_sequence)
                .unwrap(),
            &[serde_yaml_ng::Value::String(
                "/tmp:size=4096m,mode=1777".to_string()
            )]
        );
        assert!(
            dev.get("volumes")
                .and_then(serde_yaml_ng::Value::as_sequence)
                .unwrap()
                .iter()
                .filter_map(serde_yaml_ng::Value::as_str)
                .any(|mount| mount.ends_with(":/workspace:rw")),
            "/workspace must remain a host bind"
        );

        for (source, target) in [
            ("shell_history", "/home/developer/.shell_history"),
            ("managed_node_modules", "/workspace/node_modules"),
            (
                "managed_pnpm_store",
                "/home/developer/.local/share/pnpm/store",
            ),
        ] {
            let mount = volume_mount(dev, source);
            assert_eq!(
                mount.get("target").and_then(serde_yaml_ng::Value::as_str),
                Some(target)
            );
            assert_eq!(
                mount
                    .get("volume")
                    .and_then(serde_yaml_ng::Value::as_mapping)
                    .and_then(|volume| volume.get("nocopy"))
                    .and_then(serde_yaml_ng::Value::as_bool),
                Some(true)
            );
        }

        let named_volumes = yaml_mapping(&yaml, "volumes");
        assert_eq!(
            named_volumes["shell_history"]
                .get("name")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("vm_sketch-api_shell_history")
        );
        assert_eq!(
            named_volumes["managed_node_modules"]
                .get("name")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("vm_sketch-api_node_modules")
        );
        let platform_store_name = format!(
            "vm_sketch-api_linux_{}_pnpm_store",
            container_architecture()
        );
        assert_eq!(
            named_volumes["managed_pnpm_store"]
                .get("name")
                .and_then(serde_yaml_ng::Value::as_str),
            Some(platform_store_name.as_str())
        );
        assert_eq!(
            named_volumes["managed_scratch"]["labels"]
                .get("com.vm.retention")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("disposable")
        );

        let instance_rendered = compose
            .render_docker_compose_with_instance(&project_dir, "feature", &context)
            .unwrap();
        let instance_yaml: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&instance_rendered).unwrap();
        let instance_volumes = yaml_mapping(&instance_yaml, "volumes");
        assert_eq!(
            instance_volumes["managed_node_modules"]
                .get("name")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("vm_sketch-api-feature_node_modules")
        );
        assert_eq!(
            instance_volumes["managed_pnpm_store"]
                .get("name")
                .and_then(serde_yaml_ng::Value::as_str),
            Some(platform_store_name.as_str()),
            "platform stores remain shared across named instances"
        );
        assert_eq!(
            instance_volumes["managed_scratch"]
                .get("name")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("vm_sketch-api_scratch"),
            "project-scoped volumes remain shared across named instances"
        );
        assert_eq!(
            compose
                .instance_name_from_container("sketch-api-feature-dev")
                .as_deref(),
            Some("feature")
        );
        assert_eq!(compose.instance_name_from_container("sketch-api-dev"), None);
    }

    #[test]
    fn preview_redacts_environment_and_database_credentials() {
        let (_temp_dir, project_dir, generated_dir) = setup_test_env();
        let config: VmConfig = serde_yaml_ng::from_str(
            r#"
provider: docker
project:
  name: secret-project
environment:
  API_TOKEN: top-secret
host_sync:
  worktrees:
    enabled: false
services:
  postgresql:
    enabled: true
    port: 5432
"#,
        )
        .unwrap();
        let compose = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker");

        let preview = compose
            .render_docker_compose_preview(&project_dir, None, &ProviderContext::default())
            .unwrap();

        assert!(!preview.contains("top-secret"));
        assert!(!preview.contains(project_dir.to_string_lossy().as_ref()));
        assert!(preview.contains("API_TOKEN=<redacted>"));
        assert!(preview.contains("DATABASE_URL=<redacted>"));
        assert!(preview.contains("<host-path>:/workspace:rw"));
    }

    #[test]
    fn rewrites_compose_when_registry_context_changes() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let project_dir = temp_dir.path().to_path_buf();

        let mut vm_config = VmConfig::default();
        vm_config.project = Some(vm_config::config::ProjectConfig {
            name: Some("test-project".to_string()),
            ..Default::default()
        });

        let context_without_registry = ProviderContext::with_verbose(false);
        let compose_ops = ComposeOperations::new(&vm_config, &temp_path, &project_dir, "docker");
        let build_context = temp_path.join("build_context");
        std::fs::create_dir_all(&build_context).unwrap();

        let compose_path = compose_ops
            .write_docker_compose(&build_context, &context_without_registry)
            .unwrap();

        let initial_content = std::fs::read_to_string(&compose_path).unwrap();
        assert!(!initial_content.contains("NPM_CONFIG_REGISTRY="));
        assert!(!initial_content.contains("VM_CARGO_REGISTRY_HOST="));

        let mut global_config = GlobalConfig::default();
        global_config.services.package_registry.enabled = true;
        global_config.services.package_registry.port = 3080;
        let context_with_registry = ProviderContext::with_verbose(false).with_config(global_config);
        compose_ops
            .write_docker_compose(&build_context, &context_with_registry)
            .unwrap();
        let updated_content = std::fs::read_to_string(&compose_path).unwrap();

        let host = vm_platform::platform::get_host_gateway();
        assert!(updated_content.contains(&format!("NPM_CONFIG_REGISTRY=http://{host}:3080/npm/")));
        assert!(updated_content.contains(&format!("VM_CARGO_REGISTRY_HOST={host}")));
        assert!(updated_content.contains("VM_CARGO_REGISTRY_PORT=3080"));
        assert!(updated_content.contains(&format!("PIP_INDEX_URL=http://{host}:3080/pypi/simple/")));
        assert_ne!(initial_content, updated_content);

        compose_ops
            .write_docker_compose(&build_context, &context_without_registry)
            .unwrap();
        let disabled_content = std::fs::read_to_string(&compose_path).unwrap();
        assert!(!disabled_content.contains("NPM_CONFIG_REGISTRY="));
        assert!(!disabled_content.contains("VM_CARGO_REGISTRY_HOST="));
    }
}
