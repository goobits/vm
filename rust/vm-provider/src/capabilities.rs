use std::path::{Path, PathBuf};

use vm_config::config::VmConfig;
use vm_core::error::Result;

use crate::{InstanceInfo, InstanceState, ProviderContext, TempVmState, VmError, VmStatusReport};

/// Non-interactive and interactive command forms supported by a provider.
pub trait CommandProvider {
    /// Open an interactive shell in the environment.
    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()>;

    /// Execute a command and stream its output.
    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()>;

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

    /// Stream the environment logs.
    fn logs(&self, container: Option<&str>) -> Result<()>;

    /// Stream logs with provider-specific filtering options.
    fn logs_extended(
        &self,
        container: Option<&str>,
        follow: bool,
        tail: usize,
        service: Option<&str>,
        _config: &VmConfig,
    ) -> Result<()> {
        let _ = (follow, tail, service);
        self.logs(container)
    }

    /// Copy files to or from an environment.
    fn copy(&self, source: &str, destination: &str, container: Option<&str>) -> Result<()>;
}

/// Environment lifecycle, discovery, state, and ownership metadata.
pub trait InstanceProvider {
    fn name(&self) -> &'static str;

    fn create(&self, context: &ProviderContext) -> Result<()>;

    fn create_instance(&self, instance_name: &str, context: &ProviderContext) -> Result<()>;

    fn start(&self, container: Option<&str>, context: &ProviderContext) -> Result<()>;

    fn stop(&self, container: Option<&str>) -> Result<()>;

    fn destroy(&self, container: Option<&str>, context: &ProviderContext) -> Result<()>;

    fn restart(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        self.stop(container)?;
        self.start(container, context)
    }

    fn status(&self, container: Option<&str>) -> Result<VmStatusReport>;

    fn instance_state(&self, container: Option<&str>) -> Result<InstanceState>;

    fn is_ready(&self, container: Option<&str>) -> Result<bool> {
        Ok(self.instance_state(container)?.is_running())
    }

    fn is_shell_ready(&self, container: Option<&str>) -> Result<bool> {
        self.is_ready(container)
    }

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

/// Provisioning and mutable runtime reconciliation supported by a provider.
pub trait ProvisioningProvider {
    fn provision(&self, container: Option<&str>) -> Result<()>;

    fn reconcile_runtime(
        &self,
        _container: Option<&str>,
        _context: &ProviderContext,
    ) -> Result<()> {
        Ok(())
    }

    fn get_sync_directory(&self) -> String;
}

/// Temporary-VM mount and health operations, available through an explicit capability check.
pub trait TempProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()>;
    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()>;
    fn check_container_health(&self, container_name: &str) -> Result<bool>;
    fn is_container_running(&self, container_name: &str) -> Result<bool>;
}
