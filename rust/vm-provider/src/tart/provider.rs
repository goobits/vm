use super::{instance::TartInstanceManager, readiness::SharedShellProbeCache, TartCommand};
use crate::{instance::extract_project_name, VmError};
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};
use vm_config::config::VmConfig;
use vm_core::command_stream::{is_tool_installed, stream_command, stream_command_with_env};
use vm_core::error::Result;

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

mod command_impl;
mod instance_impl;
mod provisioning_impl;
#[cfg(test)]
mod tests;
