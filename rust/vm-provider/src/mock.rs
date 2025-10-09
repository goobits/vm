use crate::{Provider, TempProvider};
use std::path::Path;
use vm_config::config::VmConfig;
use vm_core::error::Result;

#[derive(Debug, Default)]
pub struct MockProvider;

impl Provider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn create(&self) -> Result<()> { Ok(()) }
    fn create_with_context(&self, _context: &crate::context::ProviderContext) -> Result<()> { Ok(()) }
    fn start(&self, _container: Option<&str>) -> Result<()> { Ok(()) }
    fn stop(&self, _container: Option<&str>) -> Result<()> { Ok(()) }
    fn destroy(&self, _container: Option<&str>) -> Result<()> { Ok(()) }
    fn ssh(&self, _container: Option<&str>, _relative_path: &Path) -> Result<()> { Ok(()) }

    fn exec(&self, _container: Option<&str>, cmd: &[String]) -> Result<()> {
        println!("Mock exec successful: {}", cmd.join(" "));
        Ok(())
    }

    fn logs(&self, _container: Option<&str>) -> Result<()> {
        println!("Mock log line 1");
        Ok(())
    }

    fn status(&self, _container: Option<&str>) -> Result<()> {
        println!("Status for mock-vm:");
        println!("  Running: true");
        println!("  IP Address: 127.0.0.1");
        Ok(())
    }

    fn restart(&self, _container: Option<&str>) -> Result<()> { Ok(()) }
    fn provision(&self, _container: Option<&str>) -> Result<()> { Ok(()) }

    fn list(&self) -> Result<()> {
        println!("{:<20} {:<10} {:<18}", "NAME", "STATUS", "IP ADDRESS");
        println!("{:<20} {:<10} {:<18}", "mock-vm-1", "running", "127.0.0.1");
        println!("{:<20} {:<10} {:<18}", "mock-vm-2", "stopped", "N/A");
        Ok(())
    }

    fn kill(&self, _container: Option<&str>) -> Result<()> { Ok(()) }
    fn get_sync_directory(&self) -> String { "/tmp/mock_sync".to_string() }
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> { Some(self) }
}

impl TempProvider for MockProvider {
    fn update_mounts(&self, _state: &crate::TempVmState) -> Result<()> { Ok(()) }
    fn recreate_with_mounts(&self, _state: &crate::TempVmState) -> Result<()> { Ok(()) }
    fn check_container_health(&self, _container_name: &str) -> Result<bool> { Ok(true) }
    fn is_container_running(&self, _container_name: &str) -> Result<bool> { Ok(true) }
}

impl MockProvider {
    pub fn new(_config: VmConfig) -> Self {
        Self::default()
    }
}