//! VM interaction command handlers
//!
//! This module provides commands for interacting with running VMs including
//! SSH connections, command execution, and log viewing.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use tracing::debug;

use crate::error::{VmError, VmResult};
use vm_cli::msg;
use vm_config::config::VmConfig;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;
use vm_provider::Provider;

/// Helper function to handle SSH start prompt interaction
fn handle_ssh_start_prompt(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    relative_path: &Path,
    vm_name: &str,
    _user: &str,
    _workspace_path: &str,
    _shell: &str,
) -> VmResult<Option<VmResult<()>>> {
    // Check if we're in an interactive terminal
    if !io::stdin().is_terminal() {
        vm_println!("{}", MESSAGES.vm_ssh_start_hint);
        return Ok(None);
    }

    print!("{}", MESSAGES.vm_ssh_start_prompt);
    io::stdout()
        .flush()
        .map_err(|e| VmError::general(e, "Failed to flush stdout"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| VmError::general(e, "Failed to read user input"))?;

    if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
        vm_println!("{}", MESSAGES.vm_ssh_start_aborted);
        return Ok(None);
    }

    // Start the VM
    vm_println!("{}", msg!(MESSAGES.vm_ssh_starting, name = vm_name));

    if let Err(e) = provider.start(container) {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_ssh_start_failed,
                name = vm_name,
                error = e.to_string()
            )
        );
        return Ok(None);
    }

    vm_println!("{}", msg!(MESSAGES.vm_ssh_reconnecting, name = vm_name));

    let retry_result = provider.ssh(container, relative_path);
    match &retry_result {
        Ok(()) => {
            vm_println!("{}", msg!(MESSAGES.vm_ssh_disconnected, name = vm_name));
        }
        Err(e) => {
            vm_println!("\n❌ SSH connection failed: {}", e);
        }
    }

    Ok(Some(retry_result.map_err(VmError::from)))
}

/// Handle SSH into VM
pub fn handle_ssh(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    path: Option<PathBuf>,
    command: Option<Vec<String>>,
    config: VmConfig,
) -> VmResult<()> {
    if let Some(cmd) = command {
        // If a command is provided, delegate to the exec handler
        return handle_exec(provider, container, cmd, config);
    }

    let relative_path = path.unwrap_or_else(|| PathBuf::from("."));
    let workspace_path = config
        .project
        .as_ref()
        .and_then(|p| p.workspace_path.as_deref())
        .unwrap_or("/workspace");

    debug!(
        "SSH command: relative_path='{}', workspace_path='{}'",
        relative_path.display(),
        workspace_path
    );

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    // Default to "developer" for user since users field may not exist
    let user = "developer";

    let shell = config
        .terminal
        .as_ref()
        .and_then(|t| t.shell.as_deref())
        .unwrap_or("zsh");

    vm_println!("{}", msg!(MESSAGES.vm_ssh_connecting, name = vm_name));

    let result = provider.ssh(container, &relative_path);

    // Show message when SSH session ends
    match &result {
        Ok(()) => {
            vm_println!("{}", msg!(MESSAGES.vm_ssh_disconnected, name = vm_name));
        }
        Err(e) => {
            let error_str = e.to_string();

            // Check if VM doesn't exist first
            if error_str.contains("No such container") || error_str.contains("No such object") {
                vm_println!("{}", msg!(MESSAGES.vm_ssh_vm_not_found, name = vm_name));

                // Offer to create the VM
                if io::stdin().is_terminal() {
                    print!("{}", MESSAGES.vm_ssh_create_prompt);
                    io::stdout().flush()?;

                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;

                    if matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                        // Actually create the VM
                        vm_println!("{}", msg!(MESSAGES.vm_ssh_creating, name = vm_name));

                        #[allow(clippy::excessive_nesting)]
                        match provider.create() {
                            Ok(()) => {
                                vm_println!(
                                    "{}",
                                    msg!(MESSAGES.vm_ssh_create_success, name = vm_name)
                                );

                                // Now try SSH again
                                return Ok(provider.ssh(container, &relative_path)?);
                            }
                            Err(create_err) => {
                                vm_println!(
                                    "{}",
                                    msg!(
                                        MESSAGES.vm_ssh_create_failed,
                                        name = vm_name,
                                        error = create_err.to_string()
                                    )
                                );
                                return Err(create_err.into());
                            }
                        }
                    } else {
                        vm_println!("\n💡 Create with: vm create");
                        vm_println!("💡 List existing VMs: vm list");
                    }
                } else {
                    vm_println!("\n💡 Create with: vm create");
                    vm_println!("💡 List existing VMs: vm list");
                }
                // Return a clean error that won't be misinterpreted by the main error handler
                return Err(VmError::vm_operation(
                    std::io::Error::new(std::io::ErrorKind::NotFound, "VM does not exist"),
                    Some(vm_name),
                    "ssh",
                ));
            }
            // Check if the error is because the container is not running
            else if error_str.contains("is not running")
                || error_str.contains("Container is not running")
                || (error_str.contains("docker")
                    && error_str.contains("exec")
                    && error_str.contains("exited with code 1")
                    && !error_str.contains("No such"))
            {
                vm_println!("{}", msg!(MESSAGES.vm_ssh_not_running, name = vm_name));

                // Handle interactive prompt
                if let Some(retry_result) = handle_ssh_start_prompt(
                    provider,
                    container,
                    &relative_path,
                    vm_name,
                    user,
                    workspace_path,
                    shell,
                )? {
                    return retry_result;
                }
            } else if error_str.contains("connection lost")
                || error_str.contains("connection failed")
            {
                vm_println!("{}", MESSAGES.vm_ssh_connection_lost);
            } else {
                // For other errors, show the actual error but clean up the message
                vm_println!("{}", MESSAGES.vm_ssh_session_ended);
            }
        }
    }

    Ok(result?)
}

/// Handle command execution in VM
pub fn handle_exec(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    command: Vec<String>,
    config: VmConfig,
) -> VmResult<()> {
    debug!(
        "Executing command in VM: command={:?}, provider='{}'",
        command,
        provider.name()
    );

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    let cmd_display = command.join(" ");
    vm_println!(
        "{}",
        msg!(
            MESSAGES.vm_exec_header,
            name = vm_name,
            command = &cmd_display
        )
    );

    let result = provider.exec(container, &command);

    match &result {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm_exec_separator);
            vm_println!("{}", MESSAGES.vm_exec_success);
        }
        Err(e) => {
            vm_println!("{}", MESSAGES.vm_exec_separator);
            vm_println!(
                "{}",
                msg!(MESSAGES.vm_exec_troubleshooting, error = e.to_string())
            );
        }
    }

    result.map_err(VmError::from)
}

/// Handle VM logs viewing
pub fn handle_logs(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
) -> VmResult<()> {
    debug!("Viewing VM logs: provider='{}'", provider.name());

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    let container_name = format!("{}-dev", vm_name);

    vm_println!("{}", msg!(MESSAGES.vm_logs_header, name = vm_name));

    let result = provider.logs(container);

    vm_println!(
        "{}",
        msg!(MESSAGES.vm_logs_footer, container = &container_name)
    );

    result.map_err(VmError::from)
}
