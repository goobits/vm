use std::path::{Path, PathBuf};

use vm_core::error::Result;

use crate::{InstanceInfo, ProviderContext, TempVmState, VmError};

/// Non-interactive and interactive command forms supported by a provider.
pub trait CommandProvider {
    fn exec_interactive(
        &self,
        _container: Option<&str>,
        _working_dir: &Path,
        _cmd: &[String],
    ) -> Result<()> {
        Err(VmError::Provider(
            "This provider does not support interactive commands".into(),
        ))
    }

    fn exec_with_stdin(
        &self,
        _container: Option<&str>,
        _cmd: &[String],
        _input: &[u8],
    ) -> Result<()> {
        Err(VmError::Provider(
            "This provider does not support command standard input".into(),
        ))
    }

    fn exec_output(&self, _container: Option<&str>, _cmd: &[String]) -> Result<String> {
        Err(VmError::Provider(
            "This provider does not support captured command output".into(),
        ))
    }
}

/// Named-instance discovery, creation, and ownership metadata.
pub trait InstanceProvider {
    fn create_instance(&self, instance_name: &str, context: &ProviderContext) -> Result<()>;

    fn resolve_instance_name(&self, instance: Option<&str>) -> Result<String> {
        Ok(instance.unwrap_or("default").to_string())
    }

    fn list_instances(&self) -> Result<Vec<InstanceInfo>>;

    fn instance_config_path(&self, _instance: &str) -> Result<Option<PathBuf>> {
        Ok(None)
    }

    fn reusable_host_ports(&self, _environment: &str) -> Result<Vec<u16>> {
        Ok(Vec::new())
    }

    fn supports_multi_instance(&self) -> bool {
        false
    }
}

/// Temporary-VM mount and health operations, available through an explicit capability check.
pub trait TempProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()>;
    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()>;
    fn check_container_health(&self, container_name: &str) -> Result<bool>;
    fn is_container_running(&self, container_name: &str) -> Result<bool>;
}
