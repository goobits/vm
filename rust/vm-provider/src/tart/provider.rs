use super::{
    instance::TartInstanceManager, provisioner::TartProvisioner, readiness::SharedShellProbeCache,
    storage, TartCommand,
};
use crate::{
    context::ProviderContext, instance::extract_project_name, project_plan::ProjectPlan,
    shell_session, CommandProvider, InstanceInfo, InstanceProvider, InstanceState, Provider,
    ProvisioningProvider, TempProvider, VmError, VmStatusReport,
};
use duct::cmd;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{error, info};
use vm_config::config::VmConfig;
use vm_core::command_stream::{is_tool_installed, stream_command, stream_command_with_env};
use vm_core::error::Result;
use vm_core::msg;
use vm_messages::messages::MESSAGES;

pub(crate) fn sanitize_log_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

pub(crate) fn tart_run_log_path(vm_name: &str) -> String {
    format!("/tmp/vm-tart-{}.log", sanitize_log_name(vm_name))
}

#[derive(Clone)]
pub struct TartProvider {
    pub(super) config: VmConfig,
    pub(super) command: TartCommand,
    pub(super) shell_probe_cache: SharedShellProbeCache,
}

impl TartProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("tart") {
            return Err(VmError::Dependency("Tart".into()));
        }
        Self::from_config(config)
    }

    fn from_config(config: VmConfig) -> Result<Self> {
        let project = extract_project_name(&config);
        let command = TartCommand::for_project(&config, project)?;
        Ok(Self {
            config,
            command,
            shell_probe_cache: Arc::new(Mutex::new(None)),
        })
    }

    pub(super) fn tart_home(&self) -> Option<String> {
        self.command
            .home()
            .map(|path| path.to_string_lossy().into_owned())
    }

    pub(super) fn tart_expr<A: AsRef<OsStr>>(&self, args: &[A]) -> duct::Expression {
        self.tart().expr(args)
    }

    pub(super) fn tart(&self) -> &TartCommand {
        &self.command
    }

    pub(super) fn stream_tart_command<A: AsRef<OsStr>>(&self, args: &[A]) -> Result<()> {
        if let Some(tart_home) = self.tart_home() {
            stream_command_with_env("tart", args, &[("TART_HOME", tart_home.as_str())])
        } else {
            stream_command("tart", args)
        }
    }

    pub(super) fn get_instance_state(&self, instance_name: &str) -> Result<Option<String>> {
        let output = self.tart_expr(&["list", "--format", "json"]).read()?;
        let vms: Vec<serde_json::Value> = serde_json::from_str(&output)?;
        for vm in vms {
            if vm["Name"] == instance_name {
                return Ok(vm["State"].as_str().map(|state| state.to_string()));
            }
        }
        Ok(None)
    }

    pub(super) fn tart_image_exists(&self, image_name: &str) -> Result<bool> {
        let output = self.tart_expr(&["list", "--format", "json"]).read()?;
        let vms: Vec<serde_json::Value> = serde_json::from_str(&output)?;
        Ok(vms.iter().any(|vm| vm["Name"].as_str() == Some(image_name)))
    }

    pub(super) fn is_instance_running(&self, instance_name: &str) -> Result<bool> {
        Ok(matches!(
            self.get_instance_state(instance_name)?.as_deref(),
            Some("running")
        ))
    }

    fn tart_state_requires_stop(state: Option<&str>) -> bool {
        matches!(state, Some("running"))
    }

    fn vm_name(&self) -> String {
        extract_project_name(&self.config).to_string()
    }

    /// Create instance manager for multi-instance operations
    pub(super) fn instance_manager(&self) -> TartInstanceManager<'_> {
        TartInstanceManager::new(&self.config, self.command.clone())
    }

    /// Resolve VM name with instance support
    pub(super) fn vm_name_with_instance(&self, instance: Option<&str>) -> Result<String> {
        match instance {
            Some(name) if self.get_instance_state(name)?.is_some() => Ok(name.to_string()),
            Some(_) => {
                let manager = self.instance_manager();
                manager.resolve_instance_name(instance)
            }
            None => Ok(self.vm_name()),
        }
    }
}

impl CommandProvider for TartProvider {
    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        self.open_shell(container, relative_path)
    }

    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        let args = self.guest_exec_args(container, cmd)?;
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.stream_tart_command(&arg_refs)
    }

    fn exec_interactive(
        &self,
        container: Option<&str>,
        working_dir: &Path,
        cmd: &[String],
    ) -> Result<()> {
        self.open_interactive_command(container, working_dir, cmd)
    }

    fn exec_with_stdin(&self, container: Option<&str>, cmd: &[String], input: &[u8]) -> Result<()> {
        let args = self.guest_exec_args(container, cmd)?;
        self.tart_expr(&args)
            .stdin_bytes(input.to_vec())
            .run()
            .map(|_| ())
            .map_err(|_| VmError::Provider("Tart guest command with standard input failed".into()))
    }

    fn exec_output(&self, container: Option<&str>, cmd: &[String]) -> Result<String> {
        let args = self.guest_exec_args(container, cmd)?;
        self.tart_expr(&args)
            .stderr_capture()
            .read()
            .map_err(|error| {
                VmError::Provider(format!(
                    "Failed to capture Tart guest command output: {error}"
                ))
            })
    }

    fn logs(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        let tart_home = self.tart_home().map(PathBuf::from).map_or_else(
            || vm_core::user_paths::home_dir().map(|home| home.join(".tart")),
            Ok,
        )?;
        let log_path = tart_home.join("vms").join(&vm_name).join("app.log");

        if !log_path.exists() {
            let error_msg = format!("Log file not found at: {}", log_path.display());
            error!("{}", error_msg);
            info!("{}", MESSAGES.service.provider_logs_unavailable);
            info!(
                "{}",
                msg!(
                    MESSAGES.service.provider_logs_expected_location,
                    name = vm_name
                )
            );
            return Err(VmError::Internal(error_msg));
        }

        info!(
            "{}",
            msg!(
                MESSAGES.service.provider_logs_showing,
                path = log_path.display().to_string()
            )
        );
        info!("{}", MESSAGES.common.press_ctrl_c_to_stop);

        let log_path = log_path.to_string_lossy();
        stream_command("tail", &["-f", &log_path])
    }

    fn copy(&self, source: &str, destination: &str, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        let (local_path, remote_path, is_upload) = if source.contains(':') {
            let parts: Vec<&str> = source.splitn(2, ':').collect();
            if parts.len() == 2 {
                (destination, parts[1], false)
            } else {
                return Err(VmError::Provider("Invalid source format".to_string()));
            }
        } else if destination.contains(':') {
            let parts: Vec<&str> = destination.splitn(2, ':').collect();
            if parts.len() == 2 {
                (source, parts[1], true)
            } else {
                return Err(VmError::Provider("Invalid destination format".to_string()));
            }
        } else {
            (source, destination, true)
        };

        if is_upload {
            let copy_cmd = format!("cat > {}", shell_session::quote_posix_argument(remote_path));
            let output = cmd!(
                "sh",
                "-c",
                format!(
                    "cat {} | tart exec {} sh -c {}",
                    shell_session::quote_posix_argument(local_path),
                    shell_session::quote_posix_argument(&vm_name),
                    shell_session::quote_posix_argument(&copy_cmd)
                )
            );
            let output = if let Some(tart_home) = self.tart_home() {
                output.env("TART_HOME", tart_home).run()
            } else {
                output.run()
            };

            output.map_err(|e| VmError::Provider(format!("Failed to copy file to VM: {}", e)))?;
        } else {
            let copy_cmd = format!("cat {}", shell_session::quote_posix_argument(remote_path));
            let result = self
                .tart_expr(&["exec", &vm_name, "sh", "-c", &copy_cmd])
                .stdout_capture()
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to read file from VM: {}", e)))?;

            std::fs::write(local_path, result.stdout)
                .map_err(|e| VmError::Provider(format!("Failed to write local file: {}", e)))?;
        }

        Ok(())
    }
}

impl InstanceProvider for TartProvider {
    fn name(&self) -> &'static str {
        "tart"
    }

    fn create(&self, context: &ProviderContext) -> Result<()> {
        let _ = context;
        self.create_vm_internal(&self.vm_name(), None, &self.config)
    }

    fn create_instance(&self, instance_name: &str, context: &ProviderContext) -> Result<()> {
        // Apply global config defaults if present, but always use the project VmConfig
        let _ = context; // Global config is not directly applicable to VM creation
        let vm_name = format!("{}-{}", self.vm_name(), instance_name);
        self.create_vm_internal(&vm_name, Some(instance_name), &self.config)
    }

    fn start(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        self.clear_shell_transport_cache();
        let vm_name = self.vm_name_with_instance(container)?;
        let state = self
            .get_instance_state(&vm_name)?
            .ok_or_else(|| VmError::NotFound(format!("Tart VM '{vm_name}' does not exist")))?;
        if context.global_config.is_some() {
            info!("Applying config updates to Tart VM");
            self.apply_runtime_config(&vm_name, &self.config)?;
        }
        if matches!(
            InstanceState::from_runtime_status(&state),
            InstanceState::Running | InstanceState::Starting
        ) {
            return Ok(());
        }

        self.start_vm_background(&vm_name)
    }

    fn stop(&self, container: Option<&str>) -> Result<()> {
        self.clear_shell_transport_cache();
        let vm_name = self.vm_name_with_instance(container)?;
        if !Self::tart_state_requires_stop(self.get_instance_state(&vm_name)?.as_deref()) {
            return Ok(());
        }

        self.stream_tart_command(&["stop", &vm_name])
    }

    fn destroy(&self, container: Option<&str>, _context: &ProviderContext) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;

        if self.is_instance_running(&vm_name).unwrap_or(false) {
            self.tart_expr(&["stop", &vm_name]).run().map_err(|e| {
                VmError::Provider(format!("Failed to stop Tart VM before delete: {e}"))
            })?;
        }

        self.stream_tart_command(&["delete", &vm_name])?;
        storage::forget_instance(&vm_name)
    }

    fn supports_multi_instance(&self) -> bool {
        true
    }

    fn resolve_instance_name(&self, instance: Option<&str>) -> Result<String> {
        if let Some(name) = instance {
            if self.get_instance_state(name)?.is_some() {
                return Ok(name.to_string());
            }
        }
        self.instance_manager().resolve_instance_name(instance)
    }

    fn list_instances(&self) -> Result<Vec<InstanceInfo>> {
        self.instance_manager().list_instances()
    }

    fn instance_config_path(&self, instance: &str) -> Result<Option<PathBuf>> {
        self.command.instance_config_path(instance)
    }

    fn status(&self, container: Option<&str>) -> Result<VmStatusReport> {
        let instance_name = self.resolve_instance_name(container)?;
        let Some(state) = self.get_instance_state(&instance_name)? else {
            return Err(VmError::NotFound(format!(
                "Tart VM '{}' does not exist",
                instance_name
            )));
        };
        let runtime_state = InstanceState::from_runtime_status(&state);

        if !runtime_state.is_running() || !self.is_guest_agent_ready(&instance_name) {
            return Ok(VmStatusReport {
                name: instance_name.clone(),
                provider: "tart".into(),
                is_running: runtime_state.is_running(),
                state: runtime_state,
                ..Default::default()
            });
        }

        let metrics = self.collect_metrics(&instance_name)?;
        Ok(VmStatusReport {
            name: instance_name,
            provider: "tart".into(),
            container_id: None,
            state: runtime_state,
            is_running: true,
            uptime: metrics.uptime,
            resources: metrics.resources,
            services: metrics.services,
            runtime: None,
        })
    }

    fn instance_state(&self, container: Option<&str>) -> Result<InstanceState> {
        let instance_name = self.resolve_instance_name(container)?;
        let state = self.get_instance_state(&instance_name)?.ok_or_else(|| {
            VmError::NotFound(format!("Tart VM '{instance_name}' does not exist"))
        })?;
        Ok(InstanceState::from_runtime_status(&state))
    }

    fn is_ready(&self, container: Option<&str>) -> Result<bool> {
        let instance_name = self.resolve_instance_name(container)?;
        let state = self.get_instance_state(&instance_name)?.ok_or_else(|| {
            VmError::NotFound(format!("Tart VM '{instance_name}' does not exist"))
        })?;
        Ok(InstanceState::from_runtime_status(&state).is_running()
            && self.is_guest_agent_ready(&instance_name))
    }

    fn is_shell_ready(&self, container: Option<&str>) -> Result<bool> {
        let instance_name = self.resolve_instance_name(container)?;
        let state = self.get_instance_state(&instance_name)?.ok_or_else(|| {
            VmError::NotFound(format!("Tart VM '{instance_name}' does not exist"))
        })?;
        Ok(InstanceState::from_runtime_status(&state).is_running()
            && self.shell_transport(&instance_name).is_some())
    }

    fn restart(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        self.stop(container)?;
        self.start(container, context)
    }
}

impl ProvisioningProvider for TartProvider {
    fn provision(&self, container: Option<&str>) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;
        let provisioner = TartProvisioner::new(
            instance_name.clone(),
            self.get_sync_directory(),
            self.command.clone(),
        );

        let project_plan = ProjectPlan::detect(&self.host_workspace_path()?, &self.config);
        provisioner.provision(&self.config, &project_plan)?;
        self.ensure_configured_mounts_ready(&instance_name)?;

        info!("Configuration applied");
        Ok(())
    }

    fn reconcile_runtime(&self, container: Option<&str>, _context: &ProviderContext) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;
        let provisioner = TartProvisioner::new(
            instance_name,
            self.get_sync_directory(),
            self.command.clone(),
        );
        provisioner.reconcile_runtime(&self.config)
    }

    fn get_sync_directory(&self) -> String {
        self.effective_sync_directory()
    }
}

impl Provider for TartProvider {
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
    }

    fn clone_box(&self) -> Box<dyn Provider> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::TartProvider;
    use crate::{tart_base, ProvisioningProvider};
    use vm_config::config::{ImageSpec, ProjectConfig, TartConfig, VmConfig, VmSettings};

    fn provider(config: VmConfig) -> TartProvider {
        TartProvider::from_config(config).unwrap()
    }

    #[test]
    fn managed_linux_alias_resolves_to_the_versioned_cache() {
        let provider = provider(VmConfig::default());
        let config = VmConfig {
            vm: Some(VmSettings {
                image: Some(ImageSpec::String(tart_base::LINUX_NAME.to_string())),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(
            provider.get_tart_image(&config).unwrap(),
            tart_base::versioned_cache_name()
        );
    }

    #[test]
    fn tart_stop_only_runs_for_running_state() {
        assert!(TartProvider::tart_state_requires_stop(Some("running")));
        assert!(!TartProvider::tart_state_requires_stop(Some("stopped")));
        assert!(!TartProvider::tart_state_requires_stop(Some("suspended")));
        assert!(!TartProvider::tart_state_requires_stop(None));
    }

    #[test]
    fn high_tart_allocations_are_detected_for_host_headroom_warning() {
        assert!(TartProvider::uses_most_of_host(Some(6), None, 8, 16 * 1024));
        assert!(TartProvider::uses_most_of_host(
            None,
            Some(12 * 1024),
            8,
            16 * 1024
        ));
        assert!(!TartProvider::uses_most_of_host(
            Some(4),
            Some(8 * 1024),
            8,
            16 * 1024
        ));
    }

    #[test]
    fn host_workspace_path_uses_loaded_config_parent() {
        let outer = tempfile::tempdir().unwrap();
        let project_dir = outer.path().join("workspace");
        std::fs::create_dir_all(&project_dir).unwrap();
        let config_path = project_dir.join("vm.yaml");
        std::fs::write(&config_path, "provider: tart\n").unwrap();

        let provider = provider(VmConfig {
            source_path: Some(config_path),
            ..Default::default()
        });

        let resolved = provider.host_workspace_path().unwrap();
        assert_eq!(resolved, project_dir.canonicalize().unwrap());
    }

    #[test]
    fn host_workspace_path_skips_outer_workspace_wrapper() {
        let temp_dir = tempfile::tempdir().unwrap();
        let outer_workspace = temp_dir.path().join("workspace");
        let inner_workspace = outer_workspace.join("workspace");
        std::fs::create_dir_all(&inner_workspace).unwrap();
        std::fs::write(inner_workspace.join("vm.yaml"), "provider: tart\n").unwrap();

        let resolved = TartProvider::normalize_host_workspace_path(&outer_workspace).unwrap();
        assert_eq!(resolved, inner_workspace.canonicalize().unwrap());
    }

    #[test]
    fn host_workspace_path_keeps_real_project_named_workspace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path().join("workspace");
        std::fs::create_dir_all(workspace.join("workspace")).unwrap();
        std::fs::write(workspace.join("vm.yaml"), "provider: tart\n").unwrap();

        let resolved = TartProvider::normalize_host_workspace_path(&workspace).unwrap();
        assert_eq!(resolved, workspace.canonicalize().unwrap());
    }

    #[test]
    fn macos_guest_uses_writable_default_workspace() {
        let provider = provider(VmConfig {
            project: Some(ProjectConfig {
                workspace_path: Some("/workspace".to_string()),
                ..Default::default()
            }),
            tart: Some(TartConfig {
                guest_os: Some("macos".to_string()),
                ssh_user: Some("admin".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert_eq!(provider.get_sync_directory(), "/Users/admin/workspace");
    }

    #[test]
    fn linux_guest_keeps_default_workspace() {
        let provider = provider(VmConfig {
            project: Some(ProjectConfig {
                workspace_path: Some("/workspace".to_string()),
                ..Default::default()
            }),
            tart: Some(TartConfig {
                guest_os: Some("linux".to_string()),
                ssh_user: Some("admin".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert_eq!(provider.get_sync_directory(), "/workspace");
    }

    #[test]
    fn macos_guest_respects_custom_workspace() {
        let provider = provider(VmConfig {
            project: Some(ProjectConfig {
                workspace_path: Some("/Volumes/work/project".to_string()),
                ..Default::default()
            }),
            tart: Some(TartConfig {
                guest_os: Some("macos".to_string()),
                ssh_user: Some("admin".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert_eq!(provider.get_sync_directory(), "/Volumes/work/project");
    }

    #[test]
    fn tart_run_includes_nested_flag_when_configured() {
        let provider = provider(VmConfig {
            tart: Some(TartConfig {
                nested: Some(true),
                guest_os: Some("linux".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert!(provider
            .build_run_args("vm-mac", &[])
            .iter()
            .any(|argument| argument == "--nested"));
    }

    #[test]
    fn tart_run_omits_nested_flag_for_macos_guests() {
        let provider = provider(VmConfig {
            tart: Some(TartConfig {
                nested: Some(true),
                guest_os: Some("macos".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert!(!provider
            .build_run_args("vm-mac", &[])
            .iter()
            .any(|argument| argument == "--nested"));
    }

    #[test]
    fn tart_run_arguments_preserve_directory_paths_without_shell_parsing() {
        let provider = provider(VmConfig::default());
        let args = provider.build_run_args(
            "vm-mac",
            &["/Users/me/project with spaces:tag=workspace".to_string()],
        );

        assert_eq!(args[0..3], ["tart", "run", "--no-graphics"]);
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--dir", "/Users/me/project with spaces:tag=workspace"]));
        assert_eq!(args.last().map(String::as_str), Some("vm-mac"));
    }
}
