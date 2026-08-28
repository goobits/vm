// Shared Docker-compatible container provider.

mod artifacts;
pub mod build;
pub mod command;
mod compose_context;
mod compose_model;
pub mod engine;
mod image_source;
mod mountpoints;
mod ownership;

#[cfg(test)]
mod build_tests;
pub mod compose;
pub mod lifecycle;
mod preview;

// Re-export the main container-provider types and functions.
pub use build::BuildOperations;
pub use command::ContainerOps;
pub use engine::ContainerEngine;
use engine::ContainerRuntime;
pub use lifecycle::LifecycleOperations;

// Standard library
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// External crates
use tera::Tera;
use vm_core::error::Result;

// Internal imports
use crate::{
    context::ProviderContext, preflight, CommandProvider, InstanceProvider, InstanceState,
    Provider, ProvisioningProvider, TempProvider, VmStatusReport,
};
use vm_config::config::VmConfig;

pub fn validate_container_environment(engine: ContainerEngine) -> Result<()> {
    engine.validate()
}

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
        let current_uid = vm_platform::platform::current_uid();
        let current_gid = vm_platform::platform::current_gid();

        let vm_settings = config.vm.as_ref();
        let project_user = vm_settings
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer");

        // Use UID/GID from config if specified, otherwise fall back to host UID/GID
        let uid = vm_settings.and_then(|vm| vm.uid).unwrap_or(current_uid);
        let gid = vm_settings.and_then(|vm| vm.gid).unwrap_or(current_gid);

        Self {
            uid,
            gid,
            username: project_user.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct ContainerProvider {
    config: VmConfig,
    project_dir: PathBuf,
    generated_dir: PathBuf,
    runtime: ContainerRuntime,
}

impl ContainerProvider {
    pub fn new(config: VmConfig, engine: ContainerEngine) -> Result<Self> {
        let project_dir = config.project_dir()?;

        let generated_dir = artifacts::project_artifacts_dir(&config, &project_dir)?;

        Ok(Self {
            config,
            project_dir,
            generated_dir,
            runtime: ContainerRuntime::new(engine),
        })
    }

    /// Helper to create LifecycleOperations instance
    fn lifecycle_ops(&self) -> LifecycleOperations<'_> {
        LifecycleOperations::with_runtime(
            &self.config,
            &self.generated_dir,
            &self.project_dir,
            self.runtime.clone(),
        )
    }
}

/// Render the provider-native Compose configuration without contacting Docker,
/// creating credentials, or mutating the project lifecycle.
pub fn render_compose_preview(
    config: &VmConfig,
    project_dir: &Path,
    instance_name: Option<&str>,
    context: &ProviderContext,
) -> Result<String> {
    let generated_dir = artifacts::project_artifacts_location(config, project_dir)?;
    let build_context = generated_dir.join("build_context");
    compose::ComposeOperations::with_runtime(
        config,
        &generated_dir,
        project_dir,
        ContainerRuntime::new(ContainerEngine::Docker),
    )
    .render_docker_compose_preview(&build_context, instance_name, context)
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

impl CommandProvider for ContainerProvider {
    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        self.lifecycle_ops()
            .ssh_into_container(container, relative_path)
    }

    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        self.lifecycle_ops().exec_in_container(container, cmd)
    }

    fn exec_interactive(
        &self,
        container: Option<&str>,
        working_dir: &Path,
        cmd: &[String],
    ) -> Result<()> {
        self.lifecycle_ops()
            .exec_interactive_in_container(container, working_dir, cmd)
    }

    fn exec_with_stdin(&self, container: Option<&str>, cmd: &[String], input: &[u8]) -> Result<()> {
        self.lifecycle_ops()
            .exec_in_container_with_stdin(container, cmd, input)
    }

    fn exec_output(&self, container: Option<&str>, cmd: &[String]) -> Result<String> {
        self.lifecycle_ops()
            .exec_in_container_output(container, cmd)
    }

    fn logs(&self, container: Option<&str>) -> Result<()> {
        self.lifecycle_ops().show_logs(container)
    }

    fn logs_extended(
        &self,
        container: Option<&str>,
        follow: bool,
        tail: usize,
        service: Option<&str>,
        config: &VmConfig,
    ) -> Result<()> {
        self.lifecycle_ops()
            .show_logs_extended(container, follow, tail, service, config)
    }

    fn copy(&self, source: &str, destination: &str, container: Option<&str>) -> Result<()> {
        let lifecycle = self.lifecycle_ops();
        let resolved_source = if source.contains(':') && !source.starts_with('/') {
            let parts: Vec<&str> = source.splitn(2, ':').collect();
            if parts.len() == 2 {
                let container_name = lifecycle.resolve_target_container(Some(parts[0]))?;
                format!("{}:{}", container_name, parts[1])
            } else {
                source.to_string()
            }
        } else {
            source.to_string()
        };
        let resolved_destination = if destination.contains(':') && !destination.starts_with('/') {
            let parts: Vec<&str> = destination.splitn(2, ':').collect();
            if parts.len() == 2 {
                let container_name = lifecycle.resolve_target_container(Some(parts[0]))?;
                format!("{}:{}", container_name, parts[1])
            } else {
                destination.to_string()
            }
        } else if !source.contains(':') && !destination.contains(':') {
            let container_name = lifecycle.resolve_target_container(container)?;
            format!("{}:{}", container_name, destination)
        } else {
            destination.to_string()
        };

        ContainerOps::copy(&self.runtime, &resolved_source, &resolved_destination)
    }
}

impl InstanceProvider for ContainerProvider {
    fn name(&self) -> &'static str {
        self.runtime.engine().name()
    }

    fn create(&self, context: &ProviderContext) -> Result<()> {
        validate_container_environment(self.runtime.engine())?;
        preflight::check_system_resources()?;
        self.lifecycle_ops().create_container(context)
    }

    fn create_instance(&self, instance_name: &str, context: &ProviderContext) -> Result<()> {
        validate_container_environment(self.runtime.engine())?;
        preflight::check_system_resources()?;
        let lifecycle = self.lifecycle_ops();
        lifecycle.create_container_with_instance(instance_name, context)
    }

    fn start(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        self.lifecycle_ops().start_container(container, context)
    }

    fn stop(&self, container: Option<&str>) -> Result<()> {
        self.lifecycle_ops().stop_container(container)
    }

    fn destroy(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        self.lifecycle_ops().destroy_container(container, context)
    }

    fn restart(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        self.lifecycle_ops().restart_container(container, context)
    }

    fn status(&self, container: Option<&str>) -> Result<VmStatusReport> {
        self.lifecycle_ops().status_report(container)
    }

    fn instance_state(&self, container: Option<&str>) -> Result<InstanceState> {
        self.lifecycle_ops().instance_state(container)
    }

    fn supports_multi_instance(&self) -> bool {
        true
    }

    fn resolve_instance_name(&self, instance: Option<&str>) -> Result<String> {
        self.lifecycle_ops().resolve_target_container(instance)
    }

    fn list_instances(&self) -> Result<Vec<crate::InstanceInfo>> {
        ownership::list_instances(&self.runtime)
    }

    fn instance_config_path(&self, instance: &str) -> Result<Option<PathBuf>> {
        ownership::instance_config_path(&self.runtime, instance)
    }

    fn reusable_host_ports(&self, environment: &str) -> Result<Vec<u16>> {
        ownership::reusable_host_ports(&self.runtime, environment)
    }
}

impl ProvisioningProvider for ContainerProvider {
    fn provision(&self, container: Option<&str>) -> Result<()> {
        self.lifecycle_ops().provision_existing(container)?;
        tracing::info!("Configuration applied");
        Ok(())
    }

    fn reconcile_runtime(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        self.lifecycle_ops().reconcile_runtime(container, context)
    }

    fn get_sync_directory(&self) -> String {
        self.lifecycle_ops().get_sync_directory()
    }
}

impl Provider for ContainerProvider {
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
    }

    fn clone_box(&self) -> Box<dyn Provider> {
        Box::new(self.clone())
    }
}

impl TempProvider for ContainerProvider {
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

#[cfg(test)]
mod tests {
    use super::ContainerEngine;
    use std::io::Write;

    #[cfg(unix)]
    #[test]
    fn validate_docker_environment_does_not_print_version_output() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let docker = dir.path().join("docker");
        let log = dir.path().join("commands.log");
        let mut file = std::fs::File::create(&docker).unwrap();
        writeln!(
            file,
            r#"#!/bin/sh
echo "$@" >> '{}'
if [ "$1" = "--version" ]; then
  echo "Docker version should stay captured"
  exit 0
fi
if [ "$1" = "ps" ]; then
  exit 0
fi
exit 1
"#,
            log.display()
        )
        .unwrap();
        drop(file);
        let mut permissions = std::fs::metadata(&docker).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&docker, permissions).unwrap();

        let engine = ContainerEngine::Docker;
        assert_eq!(engine.executable(), "docker");
        engine
            .validate_executable(docker.to_str().unwrap())
            .unwrap();
        assert_eq!(std::fs::read_to_string(log).unwrap(), "--version\nps\n");
    }
}
