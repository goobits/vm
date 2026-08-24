use crate::{
    CommandProvider, InstanceInfo, InstanceProvider, InstanceState, Provider, ProviderContext,
    ProvisioningProvider, TempProvider, VmStatusReport,
};
use std::path::Path;
use vm_config::config::VmConfig;
use vm_core::error::Result;

#[derive(Debug, Default, Clone)]
pub struct MockProvider;

impl CommandProvider for MockProvider {
    fn ssh(&self, _container: Option<&str>, _relative_path: &Path) -> Result<()> {
        Ok(())
    }

    fn exec(&self, _container: Option<&str>, cmd: &[String]) -> Result<()> {
        println!("Mock exec successful: {}", cmd.join(" "));
        Ok(())
    }

    fn logs(&self, _container: Option<&str>) -> Result<()> {
        println!("Mock log line 1");
        Ok(())
    }

    fn copy(&self, _source: &str, _destination: &str, _container: Option<&str>) -> Result<()> {
        println!("Mock copy successful");
        Ok(())
    }
}

impl InstanceProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn create(&self, _context: &ProviderContext) -> Result<()> {
        Ok(())
    }

    fn create_instance(&self, _instance_name: &str, _context: &ProviderContext) -> Result<()> {
        Ok(())
    }

    fn start(&self, _container: Option<&str>, _context: &ProviderContext) -> Result<()> {
        Ok(())
    }

    fn stop(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn destroy(&self, _container: Option<&str>, _context: &ProviderContext) -> Result<()> {
        Ok(())
    }

    fn list_instances(&self) -> Result<Vec<InstanceInfo>> {
        Ok(vec![InstanceInfo {
            name: "mock-vm".to_string(),
            id: "mock-id".to_string(),
            status: "running".to_string(),
            provider: "mock".to_string(),
            project: Some("mock".to_string()),
            uptime: None,
            created_at: None,
        }])
    }

    fn status(&self, _container: Option<&str>) -> Result<VmStatusReport> {
        Ok(VmStatusReport {
            name: "mock-vm".to_string(),
            provider: "mock".to_string(),
            state: InstanceState::Running,
            is_running: true,
            ..Default::default()
        })
    }
    fn instance_state(&self, _container: Option<&str>) -> Result<InstanceState> {
        Ok(InstanceState::Running)
    }

    fn restart(&self, _container: Option<&str>, _context: &ProviderContext) -> Result<()> {
        Ok(())
    }
}

impl ProvisioningProvider for MockProvider {
    fn provision(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn get_sync_directory(&self) -> String {
        "/tmp/mock_sync".to_string()
    }
}

impl Provider for MockProvider {
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
    }

    fn clone_box(&self) -> Box<dyn Provider> {
        Box::new(self.clone())
    }
}

impl TempProvider for MockProvider {
    fn update_mounts(&self, _state: &crate::TempVmState) -> Result<()> {
        Ok(())
    }
    fn recreate_with_mounts(&self, _state: &crate::TempVmState) -> Result<()> {
        Ok(())
    }
    fn check_container_health(&self, _container_name: &str) -> Result<bool> {
        Ok(true)
    }
    fn is_container_running(&self, _container_name: &str) -> Result<bool> {
        Ok(true)
    }
}

impl MockProvider {
    pub fn new(_config: VmConfig) -> Self {
        Self
    }
}
