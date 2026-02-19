//! VM lifecycle command handlers
//!
//! This module provides commands for managing VM lifecycle including
//! start and stop operations.

use tracing::{debug, info_span, warn};

use crate::error::{VmError, VmResult};
use vm_cli::msg;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;
use vm_provider::{Provider, ProviderContext};

use super::helpers::{
    print_vm_runtime_details, register_vm_services_helper, unregister_vm_services_helper,
};

/// Handle VM start
pub async fn handle_start(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
    no_wait: bool,
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

    // Check if VM is already running using the provider abstraction
    if let Ok(report) = provider.get_status_report(container) {
        if report.is_running {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm.start_already_running, name = vm_name)
            );
            return Ok(());
        }
    }

    // Get display name for the VM/container
    let container_name = provider
        .get_status_report(container)
        .map(|r| r.name)
        .unwrap_or_else(|_| format!("{vm_name}-dev"));

    vm_println!("{}", msg!(MESSAGES.vm.start_header, name = vm_name));

    let context = ProviderContext::with_verbose(false).with_config(global_config.clone());
    match provider.start_with_context(container, &context) {
        Ok(()) => {
            if provider.name() == "tart"
                && !no_wait
                && !wait_for_tart_running(&*provider, container)
            {
                let log_path = format!(
                    "/tmp/vm-tart-{}.log",
                    sanitize_log_name(container.unwrap_or("unknown"))
                );
                let log_tail = std::fs::read_to_string(&log_path)
                    .ok()
                    .map(|c| tail_lines(&c, 40))
                    .filter(|t| !t.is_empty());

                match log_tail {
                    Some(tail) => {
                        vm_println!("⚠️  Tart failed to start (from {}):\n{}", log_path, tail)
                    }
                    None => vm_println!("⚠️  Tart failed to start. See {}", log_path),
                }

                return Err(VmError::provider(
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Tart VM did not reach running state",
                    ),
                    provider.name(),
                    "Start failed",
                ));
            }

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

            print_vm_runtime_details(&config, false);

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

fn wait_for_tart_running(provider: &dyn Provider, container: Option<&str>) -> bool {
    use std::thread;
    use std::time::Duration;

    for _ in 0..5 {
        thread::sleep(Duration::from_secs(1));
        if let Ok(report) = provider.get_status_report(container) {
            if report.is_running {
                return true;
            }
        }
    }

    false
}

fn tail_lines(contents: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

fn sanitize_log_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
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
