// Docker provider implementation split into logical modules

pub mod build;
pub mod compose;
pub mod lifecycle;

// Re-export the main types and functions for backwards compatibility
pub use build::BuildOperations;
pub use lifecycle::LifecycleOperations;







// Standard library
use std::fs;
use std::path::{Path, PathBuf};

// External crates
use anyhow::{Context, Result};
use tera::Tera;

// Internal imports
use crate::{
    error::ProviderError,
    preflight,
    utils::is_tool_installed,
    Provider, TempProvider,
};
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
    pub fn build_args(compose_path: &Path, subcommand: &str, extra_args: &[&str]) -> Result<Vec<String>, anyhow::Error> {
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

/// Shared template engine for all Docker operations
pub(crate) fn get_template_engine() -> Result<Tera> {
    let mut tera = Tera::default();

    // Add all Docker templates
    tera.add_raw_template("docker-compose.yml", include_str!("template.yml"))?;
    tera.add_raw_template("Dockerfile", include_str!("Dockerfile.j2"))?;

    Ok(tera)
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

    fn start(&self) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.start_container()
    }

    fn stop(&self) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.stop_container()
    }

    fn destroy(&self) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.destroy_container()
    }

    fn ssh(&self, relative_path: &std::path::Path) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.ssh_into_container(relative_path)
    }

    fn exec(&self, cmd: &[String]) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.exec_in_container(cmd)
    }

    fn logs(&self) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.show_logs()
    }

    fn status(&self) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.show_status()
    }

    fn restart(&self) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.restart_container()
    }

    fn provision(&self) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.provision_existing()
    }

    fn list(&self) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.list_containers()
    }

    fn kill(&self, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.kill_container(container)
    }

    fn get_sync_directory(&self) -> Result<String> {
        let lifecycle = self.lifecycle_ops();
        lifecycle.get_sync_directory()
    }

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
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
