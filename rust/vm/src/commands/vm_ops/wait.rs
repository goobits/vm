//! Service wait command handler
//!
//! This module provides functionality to wait for services to become ready
//! before proceeding with other operations.

use std::thread::sleep;
use std::time::{Duration, Instant};
use tracing::debug;

use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::vm_println;
use vm_provider::Provider;

/// Handle service wait command
///
/// Polls service health status until all (or specified) services are ready,
/// or until the timeout is reached.
pub fn handle_wait(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    service: Option<&str>,
    timeout: u64,
    config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    // Get VM name from config
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    debug!(
        "Waiting for services: vm_name='{}', service={:?}, timeout={}s",
        vm_name, service, timeout
    );

    // Start timer
    let start = Instant::now();
    let timeout_duration = Duration::from_secs(timeout);
    let poll_interval = Duration::from_secs(2);

    let service_filter = service.map(|s| s.to_lowercase());

    vm_println!("⏳ Waiting for services to be ready...");
    if let Some(ref svc) = service_filter {
        vm_println!("   Service: {}", svc);
    } else {
        vm_println!("   Waiting for: all services");
    }
    vm_println!("   Timeout: {}s", timeout);
    vm_println!("");

    loop {
        // Check if timeout reached
        if start.elapsed() >= timeout_duration {
            vm_println!("❌ Timeout reached after {}s", timeout);
            vm_println!("   Services did not become ready in time");
            vm_println!("\n💡 Tip: Increase timeout with --timeout flag or check service logs");
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::TimedOut, "Service wait timeout"),
                format!("Services did not become ready within {}s", timeout),
            ));
        }

        // Get status report
        match provider.get_status_report(container) {
            Ok(report) => {
                // Check if container is running
                if !report.is_running {
                    vm_println!("❌ Container is not running");
                    vm_println!("   Start it with: vm start");
                    return Err(VmError::general(
                        std::io::Error::new(std::io::ErrorKind::NotFound, "Container not running"),
                        "Container must be running to wait for services".to_string(),
                    ));
                }

                // If no services configured, just check if container is running
                if report.services.is_empty() {
                    if service_filter.is_some() {
                        vm_println!("❌ No services configured");
                        vm_println!("   Add services to vm.yaml configuration");
                        return Err(VmError::general(
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "No services configured",
                            ),
                            "No services found in configuration".to_string(),
                        ));
                    } else {
                        // No specific service requested and container is running - success
                        vm_println!("✓ Container is running (no services configured)");
                        return Ok(());
                    }
                }

                // Filter services if a specific service was requested
                let services_to_check: Vec<_> = if let Some(ref filter) = service_filter {
                    report
                        .services
                        .iter()
                        .filter(|s| s.name.to_lowercase() == *filter)
                        .collect()
                } else {
                    report.services.iter().collect()
                };

                if services_to_check.is_empty() && service_filter.is_some() {
                    vm_println!(
                        "❌ Service '{}' not found",
                        service_filter.as_ref().unwrap()
                    );
                    vm_println!("   Available services:");
                    for svc in &report.services {
                        vm_println!("     • {}", svc.name);
                    }
                    return Err(VmError::general(
                        std::io::Error::new(std::io::ErrorKind::NotFound, "Service not found"),
                        format!("Service '{}' not found", service_filter.as_ref().unwrap()),
                    ));
                }

                // Check if all target services are ready
                let all_ready = services_to_check.iter().all(|s| s.is_running);

                if all_ready {
                    let elapsed = start.elapsed().as_secs();
                    vm_println!("✓ All services ready! ({}s)", elapsed);
                    for svc in services_to_check {
                        let port_info = match svc.port {
                            Some(port) => format!(" (port {})", port),
                            None => String::new(),
                        };
                        vm_println!("  🟢 {}{}", svc.name, port_info);
                    }
                    return Ok(());
                }

                // Show which services are not ready yet
                let elapsed = start.elapsed().as_secs();
                debug!("Services not ready yet ({}s elapsed):", elapsed);
                for svc in services_to_check {
                    if !svc.is_running {
                        let status = svc
                            .error
                            .as_ref()
                            .map(|e| format!("error: {}", e))
                            .unwrap_or_else(|| "starting...".to_string());
                        vm_println!("  🔴 {} ({})", svc.name, status);
                    }
                }
            }
            Err(e) => {
                debug!("Failed to get status report: {}", e);
                vm_println!("⚠️  Unable to check service status: {}", e);
                // Continue waiting - might be transient error
            }
        }

        // Wait before next poll
        sleep(poll_interval);
    }
}
