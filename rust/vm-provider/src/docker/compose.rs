use std::path::{Path, PathBuf};

// External crates
use tera::Context as TeraContext;
use vm_core::command_stream::stream_command;
use vm_core::error::{Result, VmError};

// Internal imports
use super::artifacts::{compose_path, secure_write_if_changed};
use super::build::BuildOperations;
use super::compose_context::{
    build_service_environment, configure_ssh_agent, configure_worktrees, ensure_ai_sync_dirs,
    process_dotfiles,
};
use super::compose_model::{RenderedResources, RenderedStorage};
use super::preview::redact_compose;
use super::{DockerOps, UserConfig};
use crate::guest_cache::GuestCachePolicy;
use crate::user_home::resolve_home_dir;
use crate::{Mount, ProviderContext, TempVmState};
use vm_config::config::VmConfig;

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
    ) -> Result<(VmConfig, String)> {
        if instance.is_empty()
            || !instance.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
            || matches!(instance, "." | "..")
        {
            return Err(VmError::Validation(format!(
                "Invalid Docker instance name '{instance}'"
            )));
        }
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
        Ok((custom_config, final_name))
    }

    /// Internal method that handles rendering with optional instance name
    fn render_docker_compose_internal(
        &self,
        build_context_dir: &Path,
        instance_name: Option<&str>,
        context: &ProviderContext,
        image_tag: Option<&str>,
        extra_mounts: Option<&[Mount]>,
        mode: RenderMode,
    ) -> Result<String> {
        // Use shared template engine instead of creating new instance
        let tera = super::get_compose_tera();

        let project_dir_str = BuildOperations::path_to_string(self.project_dir)?;
        let build_context_str = BuildOperations::path_to_string(build_context_dir)?;

        let user_config = UserConfig::from_vm_config(self.config);

        let mut service_environment = build_service_environment(self.config, context);
        if mode == RenderMode::Preview {
            for (_, value) in &mut service_environment {
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
            Some(instance) => self.create_instance_config(base_project_name, instance)?,
            None => (self.config.clone(), base_project_name.to_string()),
        };
        let port_binding = final_config
            .vm
            .as_ref()
            .and_then(|vm| vm.port_binding.as_deref())
            .unwrap_or("127.0.0.1");
        if mode == RenderMode::Preview {
            for value in final_config.environment.values_mut() {
                *value = "<redacted>".to_string();
            }
        }

        let guest_home_dir = format!("/home/{}", user_config.username);
        let cache_policy = GuestCachePolicy::new(base_project_name);
        let guest_cache_env = cache_policy.container_environment(&guest_home_dir, &final_config);
        let tool_cache_target = format!("{guest_home_dir}/.cache");
        let package_checkout_target = format!("{guest_home_dir}/.local/share/vm/package-checkouts");
        let storage = RenderedStorage::new(
            &final_config,
            base_project_name,
            &final_project_name,
            &tool_cache_target,
            &package_checkout_target,
        );
        let resources = RenderedResources::resolve(&final_config)?;
        let workspace_path = final_config
            .project
            .as_ref()
            .and_then(|project| project.workspace_path.as_deref())
            .unwrap_or("/workspace");
        let workspace_access = final_config
            .project
            .as_ref()
            .map(|project| project.workspace_access)
            .unwrap_or_default();
        let mut rendered_mounts = final_config
            .mounts
            .iter()
            .map(|mount| Mount::from_config(mount, self.project_dir))
            .collect::<Result<Vec<_>>>()?;
        rendered_mounts.extend(extra_mounts.unwrap_or(&[]).iter().cloned());

        let mut tera_context = TeraContext::new();
        tera_context.insert("config", &final_config);
        tera_context.insert("project_name", &final_project_name);
        tera_context.insert("base_project_name", &base_project_name);
        tera_context.insert("dev_container_name", &format!("{final_project_name}-dev"));
        tera_context.insert(
            "package_edge_container_name",
            &format!("{final_project_name}-package-edge"),
        );
        tera_context.insert(
            "postgres_container_name",
            &format!("{final_project_name}-postgres"),
        );
        tera_context.insert(
            "build_cache_image",
            &format!("{final_project_name}:buildcache"),
        );
        tera_context.insert(
            "package_edge_cache_name",
            &format!("{final_project_name}_package_edge_cache"),
        );
        tera_context.insert("storage_volumes", &storage.mounts);
        tera_context.insert("named_volumes", &storage.named_volumes);
        tera_context.insert("tmpfs_mounts", &storage.tmpfs);
        tera_context.insert("tool_cache_target", &storage.tool_cache_target);
        tera_context.insert("package_checkout_target", &storage.package_checkout_target);
        tera_context.insert("guest_cache_env", &guest_cache_env);
        tera_context.insert("resources", &resources);
        tera_context.insert("project_dir", &project_dir_str);
        if let Some(config_path) = self.config.owning_config_path() {
            let config_path = BuildOperations::path_to_string(config_path)?;
            tera_context.insert("config_path_label", &serde_json::to_string(config_path)?);
        }
        tera_context.insert("workspace_path", workspace_path);
        tera_context.insert("workspace_access", workspace_access.as_mode());
        tera_context.insert("build_context_dir", &build_context_str);
        tera_context.insert("project_uid", &user_config.uid.to_string());
        tera_context.insert("project_gid", &user_config.gid.to_string());
        tera_context.insert("project_user", &user_config.username);
        tera_context.insert("port_binding", port_binding);
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
        tera_context.insert("service_env_vars", &service_environment);
        tera_context.insert("extra_mounts", &rendered_mounts);
        tera_context.insert("is_temporary", &extra_mounts.is_some());
        if let Some(mut edge) = final_config.package_edge.clone() {
            if mode == RenderMode::Preview {
                edge.read_token = "<redacted>".to_string();
            }
            tera_context.insert("package_edge", &edge);
        }

        // AI sync flags for template
        if let Some(ai_sync) = &self
            .config
            .host_sync
            .as_ref()
            .and_then(|hs| hs.ai_tools.as_ref())
        {
            tera_context.insert("claude_sync_enabled", &ai_sync.is_claude_enabled());
            tera_context.insert(
                "antigravity_sync_enabled",
                &ai_sync.is_antigravity_enabled(),
            );
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
            self.project_dir,
            Path::new(workspace_path),
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
                VmError::Internal(format!("Failed to render docker-compose template: {e:?}"))
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
            None,
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
            None,
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
            None,
            RenderMode::Preview,
        )?;
        redact_compose(&content)
    }

    pub fn render_docker_compose_with_mounts(&self, state: &TempVmState) -> Result<String> {
        let build_ops = BuildOperations::new(self.config, self.generated_dir, self.executable);
        let build_context_dir = build_ops.prepare_compose_build_context()?;
        self.render_docker_compose_internal(
            &build_context_dir,
            None,
            &ProviderContext::default(),
            None,
            Some(&state.mounts),
            RenderMode::Runtime,
        )
    }

    pub fn start_named_with_compose(&self, container_name: &str) -> Result<()> {
        let instance_name = self.instance_name_from_container(container_name);
        let compose_path = compose_path(self.generated_dir, instance_name.as_deref());
        let container_exists =
            DockerOps::container_exists(Some(self.executable), container_name).unwrap_or(false);

        if !container_exists {
            return Err(VmError::NotFound(format!(
                "Container '{container_name}' does not exist"
            )));
        }

        // The package edge is runtime infrastructure rather than part of the
        // derived image, so a restart can add or refresh it without rebuilding
        // the worker.
        self.reconcile_package_edge(container_name)?;

        // Start existing services directly to avoid Compose name conflicts.
        let expected_services =
            DockerOps::list_managed_service_containers(Some(self.executable), container_name)?;
        for service in expected_services {
            if !DockerOps::container_exists(Some(self.executable), &service).unwrap_or(false) {
                continue;
            }
            let running =
                DockerOps::is_container_running(Some(self.executable), &service).unwrap_or(false);
            if running {
                continue;
            }
            DockerOps::start_container(Some(self.executable), &service)?;
        }

        if !compose_path.exists() {
            tracing::debug!(
                "Generated Compose file is unavailable; starting only '{}'",
                container_name
            );
        }

        DockerOps::start_container(Some(self.executable), container_name)
    }

    pub fn reconcile_package_edge(&self, container_name: &str) -> Result<()> {
        let Some(edge) = self.config.package_edge.as_ref() else {
            return Ok(());
        };
        let instance_name = self.instance_name_from_container(container_name);
        let compose_path = compose_path(self.generated_dir, instance_name.as_deref());
        if !compose_path.exists() {
            return Err(VmError::Internal(format!(
                "Generated Compose file is unavailable for package-edge reconciliation: {}",
                compose_path.display()
            )));
        }
        let edge_container = container_name.strip_suffix("-dev").map_or_else(
            || format!("{container_name}-package-edge"),
            |name| format!("{name}-package-edge"),
        );
        if package_edge_is_current(self.executable, &edge_container, &edge.revision) {
            return Ok(());
        }

        let args = super::ComposeCommand::build_args(
            &compose_path,
            "up",
            &["--detach", "--no-deps", "package-edge"],
        )?;
        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        stream_command(self.executable, &args)
    }

    pub(super) fn instance_name_from_container(&self, container_name: &str) -> Option<String> {
        let project_name = self
            .config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project");
        super::compose_model::instance_name_from_container(project_name, container_name)
    }
}

fn package_edge_is_current(executable: &str, container: &str, revision: &str) -> bool {
    let Ok(output) = std::process::Command::new(executable)
        .args([
            "inspect",
            "--format",
            "{{.State.Status}}\t{{index .Config.Labels \"com.vm.package-edge.revision\"}}",
            container,
        ])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let value = String::from_utf8_lossy(&output.stdout);
    let Some((state, installed_revision)) = value.trim().split_once('\t') else {
        return false;
    };
    state == "running" && installed_revision == revision
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::compose_model::container_architecture;
    use tempfile::TempDir;
    use vm_config::config::{
        ContainerLoggingConfig, CpuLimit, MemoryLimit, PackageEdgeConfig, ProjectConfig,
        StorageConfig, TmpfsMountConfig, VmConfig, VmSettings, VolumeMountConfig, VolumeRetention,
        VolumeScope,
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
        assert_eq!(
            labels
                .get("com.vm.role")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("environment")
        );
        assert!(!labels.contains_key("com.vm.temporary"));

        let temp_state = TempVmState::new(
            "vm-temp-dev".to_string(),
            "docker".to_string(),
            project_dir.clone(),
            false,
        );
        let temp_rendered = compose
            .render_docker_compose_with_mounts(&temp_state)
            .unwrap();
        let temp_yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&temp_rendered).unwrap();
        let temp_labels = yaml_mapping(&temp_yaml, "services")["sketch-api-dev"]["labels"]
            .as_mapping()
            .unwrap();
        assert_eq!(
            temp_labels
                .get("com.vm.temporary")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("true")
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

        let environment = dev
            .get("environment")
            .and_then(serde_yaml_ng::Value::as_sequence)
            .unwrap();
        assert!(environment
            .iter()
            .any(|value| { value.as_str() == Some("VM_IMAGE_IDENTITY=sketch-api:latest") }));
        assert!(environment.iter().any(|value| {
            value.as_str() == Some("PLAYWRIGHT_BROWSERS_PATH=/home/developer/.cache/ms-playwright")
        }));
        assert!(environment.iter().any(|value| {
            value.as_str()
                == Some("CARGO_TARGET_DIR=/home/developer/.cache/vm/cargo-target/sketch-api")
        }));
        assert!(environment.iter().any(|value| {
            value.as_str() == Some("npm_config_cache=/home/developer/.cache/node/npm")
        }));

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
        let tool_cache = volume_mount(dev, "tool_cache");
        assert_eq!(
            tool_cache
                .get("target")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("/home/developer/.cache")
        );
        assert_eq!(
            tool_cache
                .get("volume")
                .and_then(serde_yaml_ng::Value::as_mapping)
                .and_then(|volume| volume.get("nocopy"))
                .and_then(serde_yaml_ng::Value::as_bool),
            Some(false)
        );

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
        let tool_cache_name = format!(
            "vm_sketch-api_linux_{}_tool_cache",
            container_architecture()
        );
        assert_eq!(
            named_volumes["tool_cache"]
                .get("name")
                .and_then(serde_yaml_ng::Value::as_str),
            Some(tool_cache_name.as_str())
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
        assert!(matches!(
            compose.render_docker_compose_with_instance(&project_dir, "a\"b", &context),
            Err(VmError::Validation(_))
        ));
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
            instance_volumes["tool_cache"]
                .get("name")
                .and_then(serde_yaml_ng::Value::as_str),
            Some(tool_cache_name.as_str()),
            "tool caches remain shared across named instances"
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
    fn renders_read_only_package_edge_without_blocking_the_worker() {
        let (_temp_dir, project_dir, generated_dir) = setup_test_env();
        let config = VmConfig {
            provider: Some("docker".into()),
            project: Some(ProjectConfig {
                name: Some("edge-test".into()),
                ..Default::default()
            }),
            package_edge: Some(PackageEdgeConfig {
                image: "registry.example/packages:1".into(),
                internal_gateway: "http://host.docker.internal:3080".into(),
                client_gateway: "http://package-edge:3080".into(),
                read_token: "read-token".into(),
                revision: "revision-1".into(),
            }),
            ..Default::default()
        };
        let rendered = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker")
            .render_docker_compose(&project_dir, &ProviderContext::default())
            .unwrap();
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).unwrap();
        let services = yaml_mapping(&yaml, "services");
        let dev = services["edge-test-dev"].as_mapping().unwrap();
        let edge = services["package-edge"].as_mapping().unwrap();

        assert!(!dev.contains_key("depends_on"));
        assert!(dev["environment"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some("VM_MANAGED_GUEST=1")));
        assert_eq!(edge["read_only"].as_bool(), Some(true));
        assert_eq!(edge["restart"].as_str(), Some("unless-stopped"));
        assert!(edge["environment"]
            .as_mapping()
            .unwrap()
            .contains_key("PKG_SERVER_INTERNAL_GATEWAY"));
        assert!(!edge["environment"]
            .as_mapping()
            .unwrap()
            .contains_key("PKG_SERVER_PUBLISH_TOKEN"));
        assert!(yaml_mapping(&yaml, "volumes").contains_key("package_edge_cache"));

        let preview = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker")
            .render_docker_compose_preview(&project_dir, None, &ProviderContext::default())
            .unwrap();
        assert!(!preview.contains("read-token"));
    }

    #[test]
    #[cfg(unix)]
    fn package_edge_probe_requires_matching_running_revision() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let runtime = directory.path().join("runtime");
        std::fs::write(&runtime, "#!/bin/sh\nprintf 'running\\trevision-1\\n'\n").unwrap();
        let mut permissions = std::fs::metadata(&runtime).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&runtime, permissions).unwrap();

        assert!(package_edge_is_current(
            runtime.to_str().unwrap(),
            "demo-package-edge",
            "revision-1"
        ));
        assert!(!package_edge_is_current(
            runtime.to_str().unwrap(),
            "demo-package-edge",
            "revision-2"
        ));
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
    fn renders_configured_mounts_and_read_only_workspace_at_the_real_target() {
        let (_temp_dir, project_dir, generated_dir) = setup_test_env();
        std::fs::create_dir(project_dir.join("shared")).unwrap();
        let config: VmConfig = serde_yaml_ng::from_str(
            r#"
provider: docker
project:
  name: mounted-project
  workspace_path: /source
  workspace_access: read_only
mounts:
  - source: shared
    target: /packages/shared
    access: read_only
host_sync:
  worktrees:
    enabled: false
"#,
        )
        .unwrap();
        let compose = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker");

        let rendered = compose
            .render_docker_compose(&project_dir, &ProviderContext::default())
            .unwrap();
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).unwrap();
        let dev = &yaml["services"]["mounted-project-dev"];
        assert_eq!(dev["working_dir"].as_str(), Some("/source"));
        let mounts = dev["volumes"].as_sequence().unwrap();
        assert!(mounts
            .iter()
            .filter_map(|mount| mount.as_str())
            .any(|mount| { mount == format!("{}:/source:ro", project_dir.display()) }));
        assert!(mounts
            .iter()
            .filter_map(|mount| mount.as_str())
            .any(|mount| {
                mount
                    == format!(
                        "{}:/packages/shared:ro",
                        project_dir.join("shared").canonicalize().unwrap().display()
                    )
            }));
        let dependency_mount = mounts
            .iter()
            .filter_map(serde_yaml_ng::Value::as_mapping)
            .find(|mount| mount["source"] == "workspace_node_modules")
            .unwrap();
        assert_eq!(dependency_mount["target"], "/source/node_modules");
    }

    #[test]
    fn binds_all_published_ports_to_configured_address() {
        let (_temp_dir, project_dir, generated_dir) = setup_test_env();
        let config: VmConfig = serde_yaml_ng::from_str(
            r#"
provider: docker
project:
  name: bound-project
vm:
  port_binding: 127.0.0.1
ports:
  _range: [3360, 3361]
  mappings:
    - host: 4000
      guest: 80
services:
  postgresql:
    enabled: true
    port: 55432
host_sync:
  worktrees:
    enabled: false
"#,
        )
        .unwrap();
        let compose = ComposeOperations::new(&config, &generated_dir, &project_dir, "docker");

        let rendered = compose
            .render_docker_compose(&project_dir, &ProviderContext::default())
            .unwrap();
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rendered).unwrap();
        let services = yaml_mapping(&yaml, "services");
        let dev_ports = services["bound-project-dev"]["ports"]
            .as_sequence()
            .unwrap();
        let postgres_ports = services["postgres"]["ports"].as_sequence().unwrap();

        assert!(dev_ports
            .iter()
            .any(|port| { port.as_str() == Some("127.0.0.1:4000:80") }));
        assert!(dev_ports
            .iter()
            .any(|port| { port.as_str() == Some("127.0.0.1:3360:3360") }));
        assert!(dev_ports
            .iter()
            .any(|port| { port.as_str() == Some("127.0.0.1:3361:3361") }));
        assert_eq!(
            postgres_ports,
            &[serde_yaml_ng::Value::String(
                "127.0.0.1:55432:5432".to_string()
            )]
        );
    }
}
