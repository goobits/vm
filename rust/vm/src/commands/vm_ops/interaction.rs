//! VM interaction command handlers.

use std::path::PathBuf;

use tracing::debug;

use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, ConfigLoader, GlobalConfig};
use vm_core::{vm_progress, vm_success};
use vm_provider::{Provider, ProviderContext};

use super::lifecycle::{ensure_running, ensure_running_for_shell};

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

/// Start an existing environment when needed, then open an interactive shell.
pub async fn handle_ssh(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    path: Option<PathBuf>,
    config: VmConfig,
    global_config: GlobalConfig,
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
    vm_progress!("Connecting to '{vm_name}'...");
    ensure_running_for_shell(provider.as_ref(), container, &config, &global_config).await?;
    reconcile_managed_guest(
        provider.as_ref(),
        container,
        vm_name,
        &config,
        &global_config,
    )?;
    if let Err(error) = crate::commands::base::reconcile_vendor_tools_in_background(
        provider.as_ref(),
        vm_name,
        &config,
    ) {
        debug!(%error, "Could not start background vendor-tool reconciliation");
    }
    crate::commands::tools::before_shell(provider.as_ref(), vm_name, &config);
    provider
        .ssh(container, &relative_path)
        .map_err(VmError::from)
}

/// Start an existing environment when needed, then execute a command.
pub async fn handle_exec(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    command: Vec<String>,
    config: VmConfig,
    global_config: GlobalConfig,
) -> VmResult<()> {
    debug!(
        argument_count = command.len(),
        provider = provider.name(),
        "Executing command in VM"
    );

    ensure_running(provider.as_ref(), container, &config, &global_config, true).await?;
    let vm_name = container.unwrap_or_else(|| {
        config
            .project
            .as_ref()
            .and_then(|project| project.name.as_deref())
            .unwrap_or("vm-project")
    });
    reconcile_managed_guest(
        provider.as_ref(),
        container,
        vm_name,
        &config,
        &global_config,
    )?;
    provider.exec(container, &command).map_err(VmError::from)
}

fn reconcile_managed_guest(
    provider: &dyn Provider,
    container: Option<&str>,
    environment: &str,
    config: &VmConfig,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    if config.package_edge.is_some() {
        provider
            .reconcile_runtime(
                container,
                &ProviderContext::default().with_config(global_config.clone()),
            )
            .map_err(VmError::from)?;
        crate::commands::packages::reconcile_client_settings(provider, environment, config)?;
    }
    crate::commands::managed_guest::reconcile_remote_commands(provider, environment)
}

/// View environment logs.
pub fn handle_logs(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    follow: bool,
    tail: usize,
    service: Option<&str>,
) -> VmResult<()> {
    debug!(
        provider = provider.name(),
        follow, tail, service, "Viewing VM logs"
    );

    provider
        .logs_extended(container, follow, tail, service, &config)
        .map_err(VmError::from)
}

/// Copy a file to or from one environment.
pub fn handle_copy(
    provider: Box<dyn Provider>,
    source: &str,
    destination: &str,
    container: Option<&str>,
    config: VmConfig,
) -> VmResult<()> {
    let direction = if source.contains(':') { "from" } else { "to" };
    debug!(provider = provider.name(), direction, "Copying files");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    vm_progress!("Copying file {direction} environment '{vm_name}'...");

    provider
        .copy(source, destination, container)
        .map_err(VmError::from)?;
    vm_success!("File copied");
    Ok(())
}
