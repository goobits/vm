//! VM lifecycle command handlers
//!
//! This module provides commands for managing VM lifecycle including
//! start, stop, restart, and provisioning operations.

use std::process::Command;

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

    // Check if container exists and whether it is already running
    let container_status = Command::new("docker")
        .args(["inspect", "--format", "{{.State.Status}}", &container_name])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        });

    if let Some(status) = container_status {
        if status == "running" {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm.start_already_running, name = vm_name)
            );
            return Ok(());
        } else {
            // Attempt to start existing stopped container instead of failing with a compose conflict
            vm_println!(
                "💡 Found existing container '{}' (status: {}), attempting to start it...",
                container_name,
                status
            );
            let start_status = Command::new("docker")
                .args(["start", &container_name])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if start_status {
                vm_println!("{}", msg!(MESSAGES.vm.start_header, name = vm_name));
                vm_println!("{}", MESSAGES.vm.start_success);
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.vm.start_info_block,
                        status = MESSAGES.common.status_running,
                        container = &container_name
                    )
                );

                // Register services after starting the existing container
                vm_println!("{}", MESSAGES.common.configuring_services);
                register_vm_services_helper(&container_name, &config, &global_config).await?;
                vm_println!("{}", MESSAGES.common.connect_hint);
                return Ok(());
            } else {
                vm_println!(
                    "⚠️  Found existing container '{}' but failed to start it. Will attempt full start.",
                    container_name
                );
            }
        }
    }

    vm_println!("{}", msg!(MESSAGES.vm.start_header, name = vm_name));

    let context = ProviderContext::with_verbose(false).with_config(global_config.clone());
    match provider.start_with_context(container, &context) {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm.start_success);

            // Show VM details
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.start_info_block,
                    status = MESSAGES.common.status_running,
                    container = container_name
                )
            );

            // Show resources if available
            if let Some(cpus) = config.vm.as_ref().and_then(|vm| vm.cpus.as_ref()) {
                if let Some(memory) = config.vm.as_ref().and_then(|vm| vm.memory.as_ref()) {
                    // Format CPU display
                    let cpu_str = match cpus.to_count() {
                        Some(count) => count.to_string(),
                        None => "unlimited".to_string(),
                    };
                    // Format memory display
                    let mem_str = match memory.to_mb() {
                        Some(mb) if mb >= 1024 => format!("{}GB", mb / 1024),
                        Some(mb) => format!("{mb}MB"),
                        None => "unlimited".to_string(),
                    };
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.common.resources_label,
                            cpus = cpu_str,
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
                        MESSAGES.common.services_label,
                        services = services.join(", ")
                    )
                );
            }

            // Register VM services and auto-start them
            let vm_instance_name = format!("{vm_name}-dev");

            vm_println!("{}", MESSAGES.common.configuring_services);
            register_vm_services_helper(&vm_instance_name, &config, &global_config).await?;

            vm_println!("{}", MESSAGES.common.connect_hint);

            Ok(())
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.start_troubleshooting,
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

            vm_println!("{}", msg!(MESSAGES.vm.stop_header, name = vm_name));

            match provider.stop(None) {
                Ok(()) => {
                    // Unregister VM services after successful stop
                    let vm_instance_name = format!("{vm_name}-dev");

                    vm_println!("{}", MESSAGES.vm.stop_success);
                    unregister_vm_services_helper(&vm_instance_name, &global_config).await?;

                    vm_println!("{}", MESSAGES.vm.stop_restart_hint);
                    Ok(())
                }
                Err(e) => {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.vm.stop_troubleshooting,
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
                msg!(MESSAGES.vm.stop_force_header, name = container_name)
            );

            match provider.kill(Some(container_name)) {
                Ok(()) => {
                    // For force kill, still unregister services for cleanup
                    vm_println!("{}", MESSAGES.vm.stop_force_success);
                    unregister_vm_services_helper(container_name, &global_config).await?;
                    Ok(())
                }
                Err(e) => {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.vm.stop_force_troubleshooting,
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

    vm_println!("{}", msg!(MESSAGES.vm.restart_header, name = vm_name));

    // Use provider.restart_with_context() for the actual VM restart, then handle services
    let context = ProviderContext::with_verbose(false).with_config(global_config.clone());
    match provider.restart_with_context(container, &context) {
        Ok(()) => {
            // After successful restart, register services
            let vm_instance_name = format!("{vm_name}-dev");
            vm_println!("{}", MESSAGES.common.configuring_services);
            register_vm_services_helper(&vm_instance_name, &config, &global_config).await?;
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.restart_troubleshooting,
                    name = vm_name,
                    error = e.to_string()
                )
            );
            return Err(VmError::from(e));
        }
    }

    vm_println!("{}", MESSAGES.vm.restart_success);
    Ok(())
}

/// Apply configuration changes to VM
pub fn handle_apply(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "apply");
    let _enter = span.enter();
    debug!("Applying configuration changes to VM");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    vm_println!("{}", msg!(MESSAGES.vm.apply_header, name = vm_name));
    vm_println!("{}", MESSAGES.vm.apply_progress);

    match provider.provision(container) {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm.apply_success);
            vm_println!("{}", MESSAGES.vm.apply_hint);
            Ok(())
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm.apply_troubleshooting, error = e.to_string())
            );
            Err(VmError::from(e))
        }
    }
}
