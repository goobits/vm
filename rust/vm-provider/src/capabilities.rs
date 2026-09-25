use std::path::{Path, PathBuf};

use vm_config::config::VmConfig;
use vm_core::error::Result;

use crate::{
    GuestExit, GuestOutput, InstanceInfo, InstanceState, LogRecord, ProviderContext, TempVmState,
    VmError, VmStatusReport,
};

/// Per-invocation guest process settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecOptions {
    pub cwd: Option<PathBuf>,
    pub user: Option<String>,
}

impl ExecOptions {
    pub fn guest_cwd(&self, workspace: &str) -> PathBuf {
        match self.cwd.as_deref() {
            Some(path) if path.is_absolute() => path.to_path_buf(),
            Some(path) => Path::new(workspace).join(path),
            None => PathBuf::from(workspace),
        }
    }

    pub fn validate_user(&self) -> Result<()> {
        if let Some(user) = self.user.as_deref() {
            if !user
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
                || !user
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "_-.".contains(character))
            {
                return Err(VmError::Provider("Invalid guest user name".into()));
            }
        }
        Ok(())
    }
}

/// Non-interactive and interactive command forms supported by a provider.
pub trait CommandProvider {
    /// Open an interactive shell in the environment.
    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()>;

    /// Execute a command and stream its output.
    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()>;

    /// Execute a guest command with inherited output and return its exact exit status.
    fn exec_status(&self, container: Option<&str>, cmd: &[String]) -> Result<GuestExit>;

    fn exec_status_with_options(
        &self,
        container: Option<&str>,
        cmd: &[String],
        options: &ExecOptions,
    ) -> Result<GuestExit> {
        if options != &ExecOptions::default() {
            return Err(VmError::Provider(
                "This provider does not support exec --cwd or --user".into(),
            ));
        }
        self.exec_status(container, cmd)
    }

    fn exec_capture_with_options(
        &self,
        _container: Option<&str>,
        _cmd: &[String],
        _options: &ExecOptions,
    ) -> Result<GuestOutput> {
        Err(VmError::Provider(
            "This provider does not support captured fleet execution".into(),
        ))
    }

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

    /// Stream typed, byte-preserving log records to a structured-output client.
    fn logs_records(
        &self,
        _container: Option<&str>,
        _follow: bool,
        _tail: usize,
        _service: Option<&str>,
        _config: &VmConfig,
        _sink: &mut dyn FnMut(LogRecord) -> Result<()>,
    ) -> Result<()> {
        Err(VmError::Provider(
            "This provider does not support structured log output".into(),
        ))
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

    /// Explain configuration drift that requires runtime recreation.
    fn runtime_drift(&self, _instance: &str) -> Result<Option<String>> {
        Ok(None)
    }

    fn supports_runtime_drift_detection(&self) -> bool {
        false
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

/// Ephemeral TCP relay operations exposed only by supporting providers.
pub trait TunnelProvider {
    fn start_tcp_relay(
        &self,
        relay_name: &str,
        local_address: &str,
        host_port: u16,
        target_instance: &str,
        remote_host: &str,
        target_port: u16,
    ) -> Result<String>;

    fn relay_is_running(&self, relay_id: &str) -> bool;

    fn stop_relay(&self, relay_id: &str) -> Result<()>;
}

#[cfg(test)]
mod exec_options_tests {
    use super::ExecOptions;
    use std::path::{Path, PathBuf};

    #[test]
    fn guest_working_directory_uses_workspace_for_relative_paths() {
        let mut options = ExecOptions::default();
        assert_eq!(options.guest_cwd("/workspace"), Path::new("/workspace"));
        options.cwd = Some(PathBuf::from("src"));
        assert_eq!(options.guest_cwd("/workspace"), Path::new("/workspace/src"));
        options.cwd = Some(PathBuf::from("/tmp"));
        assert_eq!(options.guest_cwd("/workspace"), Path::new("/tmp"));
    }

    #[test]
    fn guest_user_cannot_be_an_option_or_empty() {
        let mut options = ExecOptions::default();
        for user in ["", "--help", "name with space"] {
            options.user = Some(user.into());
            assert!(options.validate_user().is_err());
        }
        options.user = Some("build_user".into());
        assert!(options.validate_user().is_ok());
    }
}
