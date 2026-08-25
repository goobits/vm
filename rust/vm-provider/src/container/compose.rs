use std::path::{Path, PathBuf};

// External crates
use tera::Context as TeraContext;
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
use super::UserConfig;
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
    #[cfg(test)]
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

    /// Render docker-compose.yml with custom instance name
    #[cfg(test)]
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
}

#[cfg(test)]
#[path = "compose_tests.rs"]
mod tests;
