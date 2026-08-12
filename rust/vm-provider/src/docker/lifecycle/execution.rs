//! Container lifecycle execution (start/stop/restart/kill)
use super::LifecycleOperations;
use crate::{
    audio::MacOSAudioManager,
    context::ProviderContext,
    docker::{compose::ComposeOperations, DockerOps},
    InstanceState,
};
use tracing::{info, warn};
use vm_core::{
    command_stream::stream_command,
    error::{Result, VmError},
};

impl<'a> LifecycleOperations<'a> {
    #[must_use = "runtime reconciliation results should be handled"]
    pub fn reconcile_runtime(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        let target_container = self.resolve_probe_target(container)?;
        if !matches!(
            self.instance_state_for_name(&target_container)?,
            InstanceState::Running
        ) {
            return Err(VmError::Provider(format!(
                "Cannot reconcile runtime infrastructure for non-running container '{target_container}'"
            )));
        }
        if self.config.package_edge.is_none() {
            return Ok(());
        }
        let compose_ops = self.regenerate_compose_with_context(container, context)?;
        compose_ops.reconcile_package_edge(&target_container)
    }

    #[must_use = "container start results should be handled"]
    pub fn start_container(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        let target_container = self.resolve_probe_target(container)?;
        match self.instance_state_for_name(&target_container)? {
            InstanceState::Running | InstanceState::Starting => return Ok(()),
            InstanceState::Paused => {
                return DockerOps::unpause_container(Some(self.executable), &target_container);
            }
            InstanceState::Stopped | InstanceState::Suspended => {}
            InstanceState::Unknown(state) => {
                return Err(VmError::Provider(format!(
                    "Cannot start container '{target_container}' from unknown state '{state}'"
                )));
            }
        }

        if context.global_config.is_none() {
            return DockerOps::start_container(Some(self.executable), &target_container);
        }

        let compose_ops = self.regenerate_compose_with_context(container, context)?;
        compose_ops.start_named_with_compose(&target_container)
    }

    #[must_use = "container stop results should be handled"]
    pub fn stop_container(&self, container: Option<&str>) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;
        // Let Docker honor the container's configured stop timeout.
        duct::cmd(self.executable, &["stop", &target_container])
            .run()
            .map_err(|e| {
                VmError::Internal(format!(
                    "Failed to stop container '{target_container}': {e}"
                ))
            })?;
        Ok(())
    }

    #[must_use = "container destruction results should be handled"]
    pub fn destroy_container(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        let target_container = self.resolve_target_container(container)?;

        // Check if container exists before attempting destruction
        if !DockerOps::container_exists(Some(self.executable), &target_container).unwrap_or(false) {
            return Err(VmError::Internal(format!(
                "Container '{target_container}' does not exist"
            )));
        }

        // Remove the main dev container
        let result = stream_command(self.executable, &["rm", "-f", &target_container]);

        // Optionally remove service containers based on context
        if !context.preserve_services {
            info!("Removing service containers");
            let compose_ops = ComposeOperations::new(
                self.config,
                self.generated_dir,
                self.project_dir,
                self.executable,
            );
            let instance = compose_ops.instance_name_from_container(&target_container);
            let expected_services =
                compose_ops.get_expected_service_containers(instance.as_deref());

            for service_name in expected_services {
                let exists = DockerOps::container_exists(Some(self.executable), &service_name)
                    .unwrap_or(false);
                if !exists {
                    continue;
                }

                info!("Removing service container: {}", service_name);
                if let Err(e) =
                    DockerOps::remove_container(Some(self.executable), &service_name, true)
                {
                    warn!("Failed to remove service container {}: {}", service_name, e);
                }
            }
        } else {
            info!("Preserving service containers");
        }

        // Only cleanup audio if it was enabled in the configuration
        if let Some(audio_service) = self.config.services.get("audio") {
            if audio_service.enabled {
                #[cfg(target_os = "macos")]
                if let Err(e) = MacOSAudioManager::cleanup() {
                    vm_core::vm_warning!("Audio cleanup warning: {}", e);
                }
                #[cfg(not(target_os = "macos"))]
                MacOSAudioManager::cleanup();
            }
        }

        result
    }

    #[must_use = "container restart results should be handled"]
    pub fn restart_container(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        self.stop_container(container)?;
        self.start_container(container, context)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use vm_config::config::{PackageEdgeConfig, ProjectConfig, VmConfig};
    use vm_config::GlobalConfig;

    fn fake_runtime_with_edge(
        temp_dir: &TempDir,
        inspect_state: Option<&str>,
        edge_revision: Option<&str>,
    ) -> (PathBuf, PathBuf) {
        let executable = temp_dir.path().join("runtime");
        let log = temp_dir.path().join("commands.log");
        let inspect = inspect_state.map_or_else(
            || "echo 'Error: No such object' >&2; exit 1".to_string(),
            |state| {
                edge_revision.map_or_else(
                    || format!("echo '{state}'; exit 0"),
                    |revision| {
                        format!(
                            "case \"$*\" in *package-edge.revision*) printf 'running\\t{revision}\\n' ;; *) echo '{state}' ;; esac; exit 0"
                        )
                    },
                )
            },
        );
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\nif [ \"$1\" = inspect ]; then {inspect}; fi\nexit 0\n",
                log.display()
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        (executable, log)
    }

    fn fake_runtime(temp_dir: &TempDir, inspect_state: Option<&str>) -> (PathBuf, PathBuf) {
        fake_runtime_with_edge(temp_dir, inspect_state, None)
    }

    fn config() -> VmConfig {
        VmConfig {
            project: Some(ProjectConfig {
                name: Some("demo".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn missing_start_never_falls_through_to_creation() {
        let temp_dir = TempDir::new().unwrap();
        let (executable, log) = fake_runtime(&temp_dir, None);
        let config = config();
        let generated_dir = temp_dir.path().join("generated");
        let project_dir = temp_dir.path().join("project");
        let ops = LifecycleOperations::new(
            &config,
            &generated_dir,
            &project_dir,
            executable.to_str().unwrap(),
        );
        let context = ProviderContext::default().with_config(GlobalConfig::default());

        let error = ops.start_container(None, &context).unwrap_err();

        assert!(matches!(error, VmError::NotFound(_)));
        let commands = fs::read_to_string(log).unwrap();
        assert_eq!(commands.lines().count(), 1);
        assert!(commands.starts_with("inspect "));
        assert!(!commands.contains(" up "));
        assert!(!commands.contains("start "));
    }

    #[test]
    fn stopped_start_uses_runtime_start_without_printing_runtime_output() {
        let temp_dir = TempDir::new().unwrap();
        let (executable, log) = fake_runtime(&temp_dir, Some("exited"));
        let config = config();
        let generated_dir = temp_dir.path().join("generated");
        let project_dir = temp_dir.path().join("project");
        let ops = LifecycleOperations::new(
            &config,
            &generated_dir,
            &project_dir,
            executable.to_str().unwrap(),
        );

        ops.start_container(None, &ProviderContext::default())
            .unwrap();

        let commands = fs::read_to_string(log).unwrap();
        assert!(commands.lines().next().unwrap().starts_with("inspect "));
        assert!(commands.lines().any(|line| line == "start demo-dev"));
        assert!(!commands.contains("compose"));
    }

    #[test]
    fn running_runtime_reconciliation_updates_only_the_package_edge() {
        let temp_dir = TempDir::new().unwrap();
        let (executable, log) = fake_runtime(&temp_dir, Some("running"));
        let mut config = config();
        config.package_edge = Some(PackageEdgeConfig {
            image: "registry.example/edge:1".into(),
            internal_gateway: "http://host.docker.internal:3080".into(),
            client_gateway: "http://package-edge:3080".into(),
            read_token: "read-token".into(),
            revision: "revision-1".into(),
        });
        let generated_dir = temp_dir.path().join("generated");
        let project_dir = temp_dir.path().join("project");
        fs::create_dir_all(&project_dir).unwrap();
        let ops = LifecycleOperations::new(
            &config,
            &generated_dir,
            &project_dir,
            executable.to_str().unwrap(),
        );

        ops.reconcile_runtime(
            Some("demo-dev"),
            &ProviderContext::default().with_config(GlobalConfig::default()),
        )
        .unwrap();

        let commands = fs::read_to_string(log).unwrap();
        assert!(commands.contains("compose"));
        assert!(commands.contains("up --detach --no-deps package-edge"));
        assert!(!commands.contains(" build"));
        assert!(!commands.contains(" rm"));
        assert!(!commands.contains(" down"));
    }

    #[test]
    fn current_runtime_reconciliation_is_a_no_op() {
        let temp_dir = TempDir::new().unwrap();
        let (executable, log) =
            fake_runtime_with_edge(&temp_dir, Some("running"), Some("revision-1"));
        let mut config = config();
        config.package_edge = Some(PackageEdgeConfig {
            image: "registry.example/edge:1".into(),
            internal_gateway: "http://host.docker.internal:3080".into(),
            client_gateway: "http://package-edge:3080".into(),
            read_token: "read-token".into(),
            revision: "revision-1".into(),
        });
        let generated_dir = temp_dir.path().join("generated");
        let project_dir = temp_dir.path().join("project");
        fs::create_dir_all(&project_dir).unwrap();
        let ops = LifecycleOperations::new(
            &config,
            &generated_dir,
            &project_dir,
            executable.to_str().unwrap(),
        );

        ops.reconcile_runtime(
            Some("demo-dev"),
            &ProviderContext::default().with_config(GlobalConfig::default()),
        )
        .unwrap();

        let commands = fs::read_to_string(log).unwrap();
        assert!(commands.contains("package-edge.revision"));
        assert!(!commands.contains("compose"));
    }
}
