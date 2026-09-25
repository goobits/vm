use super::TartProvider;
use crate::tart::storage;
use crate::{
    context::ProviderContext, InstanceInfo, InstanceProvider, InstanceState, VmError,
    VmStatusReport,
};
use std::path::PathBuf;
use tracing::info;
use vm_core::error::Result;

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

    fn runtime_drift(&self, instance: &str) -> Result<Option<String>> {
        storage::runtime_drift(instance, self.command.home(), &self.config)
    }

    fn supports_runtime_drift_detection(&self) -> bool {
        true
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
