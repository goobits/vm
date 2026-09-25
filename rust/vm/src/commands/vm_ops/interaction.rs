//! VM interaction command handlers.

use std::path::PathBuf;

use tracing::debug;

use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, ConfigLoader, GlobalConfig};
use vm_core::vm_progress;
use vm_provider::{ExecOptions, GuestExit, InstanceState, Provider};

fn detected_relative_path(path: Option<PathBuf>) -> PathBuf {
    if let Some(path) = path {
        return path;
    }

    match ConfigLoader::new().relative_path_from_config() {
        Ok(Some(path)) => {
            debug!(path = %path.display(), "Detected path relative to vm.yaml");
            path
        }
        Ok(None) => PathBuf::from("."),
        Err(error) => {
            debug!(%error, "Could not detect path relative to vm.yaml");
            PathBuf::from(".")
        }
    }
}

/// Open an interactive shell in an existing, running environment.
pub async fn handle_ssh(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    path: Option<PathBuf>,
    config: VmConfig,
) -> VmResult<()> {
    let relative_path = detected_relative_path(path);
    let vm_name = container.unwrap_or_else(|| {
        config
            .project
            .as_ref()
            .and_then(|project| project.name.as_deref())
            .unwrap_or("vm-project")
    });

    debug!(
        provider = provider.name(),
        target = ?container,
        relative_path = %relative_path.display(),
        "Connecting to VM"
    );
    if provider.instance_state(container).map_err(VmError::from)? != InstanceState::Running
        || !provider.is_shell_ready(container).map_err(VmError::from)?
    {
        return Err(VmError::conflict(
            "Environment is not ready for a shell",
            Some("Start it with `vm start` before using `vm shell`"),
        ));
    }
    vm_progress!("Connecting to '{vm_name}'...");
    if let Err(error) = crate::commands::tools::schedule(vm_name) {
        debug!(%error, "Could not schedule background guest reconciliation");
    }
    debug!("Handing off interactive shell");
    provider
        .ssh(container, &relative_path)
        .map_err(VmError::from)
}

/// Execute a command in a running environment.
pub async fn handle_exec_with_options(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    command: Vec<String>,
    config: VmConfig,
    global_config: GlobalConfig,
    options: ExecOptions,
) -> VmResult<GuestExit> {
    debug!(
        argument_count = command.len(),
        provider = provider.name(),
        "Executing command in VM"
    );

    if provider.instance_state(container).map_err(VmError::from)? != InstanceState::Running {
        return Err(VmError::conflict(
            "Environment is not running",
            Some("Start it with `vm start` before using `vm exec`"),
        ));
    }
    let vm_name = container.unwrap_or_else(|| {
        config
            .project
            .as_ref()
            .and_then(|project| project.name.as_deref())
            .unwrap_or("vm-project")
    });
    crate::commands::tools::reconcile_managed_guest(
        provider.as_ref(),
        container,
        vm_name,
        &config,
        &global_config,
    )?;
    provider
        .exec_status_with_options(container, &command, &options)
        .map_err(VmError::from)
}
