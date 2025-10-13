//! VM lifecycle command handlers
//!
//! This module provides commands for managing VM lifecycle including
//! start, stop, restart, and provisioning operations.

use tracing::{debug, info_span, warn};

use crate::error::{VmError, VmResult};
use vm_cli::msg;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;
use vm_provider::{Provider, ProviderContext};

use super::helpers::{register_vm_services_helper, unregister_vm_services_helper};

/// Handle VM start
pub async fn handle_start(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "start");
    let _enter = span.enter();
    debug!("Starting VM");

    // Get VM name from config
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    let container_name = format!("{vm_name}-dev");

    // Check if container exists and is running
    // We need to check using Docker directly since provider.status() just shows status
    let container_exists = std::process::Command::new("docker")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output()
        .ok()
        .and_then(|output| {
            let names = String::from_utf8_lossy(&output.stdout);
            if names.lines().any(|name| name.trim() == container_name) {
                Some(())
            } else {
                None
            }
        })
        .is_some();

    if container_exists {
        // Check if it's actually running
        let is_running = std::process::Command::new("docker")
            .args(["inspect", "--format", "{{.State.Status}}", &container_name])
            .output()
            .ok()
            .and_then(|output| {
                let status = String::from_utf8_lossy(&output.stdout);
                if status.trim() == "running" {
                    Some(())
                } else {
                    None
                }
            })
            .is_some();

        if is_running {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm_start_already_running, name = vm_name)
            );
            return Ok(());
        }
    }

    vm_println!("{}", msg!(MESSAGES.vm_start_header, name = vm_name));

    let context = ProviderContext::with_verbose(false).with_config(global_config.clone());
    match provider.start_with_context(container, &context) {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm_start_success);

            // Show VM details
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_start_info_block,
                    status = MESSAGES.common_status_running,
                    container = container_name
                )
            );

            // Show resources if available
            if let Some(cpus) = config.vm.as_ref().and_then(|vm| vm.cpus) {
                if let Some(memory) = config.vm.as_ref().and_then(|vm| vm.memory.as_ref()) {
                    // Format memory display
                    let mem_str = match memory.to_mb() {
                        Some(mb) if mb >= 1024 => format!("{}GB", mb / 1024),
                        Some(mb) => format!("{mb}MB"),
                        None => format!("{memory:?}"),
                    };
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.common_resources_label,
                            cpus = cpus.to_string(),
                            memory = mem_str
                        )
                    );
                }
            }

            // Show services if any are configured
            let services: Vec<String> = config
                .services
                .iter()
                .filter(|(_, svc)| svc.enabled)
                .map(|(name, _)| name.clone())
                .collect();

            if !services.is_empty() {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.common_services_label,
                        services = services.join(", ")
                    )
                );
            }

            // Register VM services and auto-start them
            let vm_instance_name = format!("{vm_name}-dev");

            vm_println!("{}", MESSAGES.common_configuring_services);
            register_vm_services_helper(&vm_instance_name, &config, &global_config).await?;

            vm_println!("{}", MESSAGES.common_connect_hint);

            Ok(())
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_start_troubleshooting,
                    name = vm_name,
                    error = e.to_string(),
                    container = container_name
                )
            );
            Err(VmError::from(e))
        }
    }
}

/// Handle VM stop - graceful stop for current project or force kill specific container
pub async fn handle_stop(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
) -> VmResult<()> {
    match container {
        None => {
            // Graceful stop of current project VM
            let span = info_span!("vm_operation", operation = "stop");
            let _enter = span.enter();
            debug!("Stopping VM");

            let vm_name = config
                .project
                .as_ref()
                .and_then(|p| p.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("vm-project");

            vm_println!("{}", msg!(MESSAGES.vm_stop_header, name = vm_name));

            match provider.stop(None) {
                Ok(()) => {
                    // Unregister VM services after successful stop
                    let vm_instance_name = format!("{vm_name}-dev");

                    vm_println!("{}", MESSAGES.vm_stop_success);
                    unregister_vm_services_helper(&vm_instance_name, &global_config).await?;

                    vm_println!("{}", MESSAGES.vm_stop_restart_hint);
                    Ok(())
                }
                Err(e) => {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.vm_stop_troubleshooting,
                            name = vm_name,
                            error = e.to_string()
                        )
                    );
                    Err(VmError::from(e))
                }
            }
        }
        Some(container_name) => {
            // Force kill specific container
            let span = info_span!("vm_operation", operation = "kill");
            let _enter = span.enter();
            warn!("Force killing container: {}", container_name);

            vm_println!(
                "{}",
                msg!(MESSAGES.vm_stop_force_header, name = container_name)
            );

            match provider.kill(Some(container_name)) {
                Ok(()) => {
                    // For force kill, still unregister services for cleanup
                    vm_println!("{}", MESSAGES.vm_stop_force_success);
                    unregister_vm_services_helper(container_name, &global_config).await?;
                    Ok(())
                }
                Err(e) => {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.vm_stop_force_troubleshooting,
                            error = e.to_string()
                        )
                    );
                    Err(VmError::from(e))
                }
            }
        }
    }
}

/// Handle VM restart
pub async fn handle_restart(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "restart");
    let _enter = span.enter();
    debug!("Restarting VM");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    vm_println!("{}", msg!(MESSAGES.vm_restart_header, name = vm_name));

    // Use provider.restart_with_context() for the actual VM restart, then handle services
    let context = ProviderContext::with_verbose(false).with_config(global_config.clone());
    match provider.restart_with_context(container, &context) {
        Ok(()) => {
            // After successful restart, register services
            let vm_instance_name = format!("{vm_name}-dev");
            vm_println!("{}", MESSAGES.common_configuring_services);
            register_vm_services_helper(&vm_instance_name, &config, &global_config).await?;
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_restart_troubleshooting,
                    name = vm_name,
                    error = e.to_string()
                )
            );
            return Err(VmError::from(e));
        }
    }

    vm_println!("{}", MESSAGES.vm_restart_success);
    Ok(())
}

/// Handle VM provisioning
pub fn handle_provision(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "provision");
    let _enter = span.enter();
    debug!("Re-running VM provisioning");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    vm_println!("{}", msg!(MESSAGES.vm_provision_header, name = vm_name));
    vm_println!("{}", MESSAGES.vm_provision_progress);

    match provider.provision(container) {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm_provision_success);
            vm_println!("{}", MESSAGES.vm_provision_hint);
            Ok(())
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm_provision_troubleshooting, error = e.to_string())
            );
            Err(VmError::from(e))
        }
    }
}
