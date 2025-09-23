use crate::{Provider, TempProvider};
use anyhow::Result;
use std::path::Path;
use vm_config::config::VmConfig;

#[derive(Debug)]
pub struct MockProvider;

impl Provider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn create(&self) -> Result<()> {
        Ok(())
    }

    fn start(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn stop(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn destroy(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn ssh(&self, _container: Option<&str>, _relative_path: &Path) -> Result<()> {
        Ok(())
    }

    fn exec(&self, _container: Option<&str>, _cmd: &[String]) -> Result<()> {
        Ok(())
    }

    fn logs(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn status(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn restart(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn provision(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn list(&self) -> Result<()> {
        Ok(())
    }

    fn kill(&self, _container: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn get_sync_directory(&self) -> String {
        "/tmp/mock_sync".to_string()
    }

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
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
