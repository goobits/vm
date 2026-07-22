//! VM interaction command handlers.

use std::path::PathBuf;

use tracing::debug;

use crate::error::{VmError, VmResult};
use vm_cli::msg;
use vm_config::{config::VmConfig, ConfigLoader};
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;
use vm_provider::Provider;

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

fn create_command(provider: &dyn Provider) -> String {
    match provider.name() {
        "docker" | "podman" | "tart" => format!("vm create {}", provider.name()),
        _ => "vm create".to_string(),
    }
}

fn start_command(provider: &dyn Provider) -> String {
    match provider.name() {
        "docker" | "podman" | "tart" => format!("vm start {}", provider.name()),
        _ => "vm start".to_string(),
    }
}

/// Connect to a running environment. Creation, startup, and mount changes are
/// intentionally owned by their lifecycle commands.
pub fn handle_ssh(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    path: Option<PathBuf>,
    config: VmConfig,
) -> VmResult<()> {
    let relative_path = detected_relative_path(path);
    let vm_name = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");

    debug!(
        provider = provider.name(),
        target = ?container,
        relative_path = %relative_path.display(),
        "Connecting to VM"
    );
    vm_println!("{}", msg!(MESSAGES.vm.ssh_connecting, name = vm_name));

    let result = provider.ssh(container, &relative_path);
    match &result {
        Ok(()) => vm_println!("{}", msg!(MESSAGES.vm.ssh_disconnected, name = vm_name)),
        Err(error) => {
            let message = error.to_string();
            if message.contains("No such container")
                || message.contains("No such object")
                || message.contains("No container found matching")
            {
                vm_println!("{}", msg!(MESSAGES.vm.ssh_vm_not_found, name = vm_name));
                vm_println!(
                    "Create it explicitly with: {}",
                    create_command(provider.as_ref())
                );
            } else if message.contains("is not running")
                || message.contains("Container is not running")
            {
                vm_println!("{}", msg!(MESSAGES.vm.ssh_not_running, name = vm_name));
                vm_println!(
                    "Start it explicitly with: {}",
                    start_command(provider.as_ref())
                );
            } else {
                vm_println!("{}", MESSAGES.vm.ssh_session_ended);
            }
        }
    }

    result.map_err(VmError::from)
}

/// Execute a command in a running environment.
pub fn handle_exec(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    command: Vec<String>,
    config: VmConfig,
) -> VmResult<()> {
    debug!(
        command = ?command,
        provider = provider.name(),
        "Executing command in VM"
    );

    let vm_name = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let command_display = command.join(" ");
    vm_println!(
        "{}",
        msg!(
            MESSAGES.vm.exec_header,
            name = vm_name,
            command = &command_display
        )
    );

    let result = provider.exec(container, &command);
    vm_println!("{}", MESSAGES.vm.exec_separator);
    match &result {
        Ok(()) => vm_println!("{}", MESSAGES.vm.exec_success),
        Err(error) => vm_println!(
            "{}",
            msg!(MESSAGES.vm.exec_troubleshooting, error = error.to_string())
        ),
    }

    result.map_err(VmError::from)
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
    config: VmConfig,
) -> VmResult<()> {
    debug!(
        source,
        destination,
        provider = provider.name(),
        "Copying files"
    );

    let vm_name = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let direction = if source.contains(':') { "from" } else { "to" };
    vm_println!("Copying file {} VM '{}'...", direction, vm_name);

    let result = provider.copy(source, destination, None);
    match &result {
        Ok(()) => vm_println!("File copied successfully"),
        Err(error) => vm_println!("Copy failed: {}", error),
    }

    result.map_err(VmError::from)
}
