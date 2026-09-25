use super::TartProvider;
use crate::tart::provisioner::TartProvisioner;
use crate::{
    context::ProviderContext, project_plan::ProjectPlan, InstanceProvider, Provider,
    ProvisioningProvider, TempProvider,
};
use tracing::info;
use vm_core::error::Result;

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
