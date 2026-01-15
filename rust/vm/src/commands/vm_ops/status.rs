//! VM status reporting and display
//!
//! This module provides comprehensive status reporting for VMs with
//! resource usage, service health, and state information.

use tracing::debug;

use super::list::handle_list_enhanced;
use crate::error::VmResult;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::vm_println;
use vm_provider::{Provider, VmStatusReport};

/// Handle VM status check with enhanced dashboard
pub fn handle_status(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    if container.is_none() {
        return handle_list_enhanced(None);
    }

    // Get VM name from config
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    debug!(
        "Status check: vm_name='{}', provider='{}'",
        vm_name,
        provider.name()
    );

    // Get comprehensive status report
    match provider.get_status_report(container) {
        Ok(report) => {
            display_status_dashboard(&report);
            Ok(())
        }
        Err(e) => {
            debug!("Status report failed: {}, falling back to basic status", e);
            // Fallback to basic stopped status display for providers that don't support enhanced status
            display_basic_stopped_status(vm_name, provider.name());
            Ok(()) // Don't propagate error, just show status
        }
    }
}

/// Display the compact status dashboard
fn display_status_dashboard(report: &VmStatusReport) {
    // Header with VM name
    vm_println!("🖥️  {} ({})", report.name, report.provider);

    // Status line with uptime
    let status_icon = if report.is_running { "🟢" } else { "🔴" };
    let status_text = if report.is_running {
        "Running"
    } else {
        "Stopped"
    };

    if let Some(uptime) = &report.uptime {
        vm_println!("   {} {} • Uptime: {}", status_icon, status_text, uptime);
    } else {
        vm_println!("   {} {}", status_icon, status_text);
    }

    // Container ID (shortened)
    if let Some(id) = &report.container_id {
        let short_id = if id.len() > 12 { &id[..12] } else { id };
        vm_println!("   📦 {}", short_id);
    }

    // Resource usage (if available)
    if report.is_running && has_resource_data(&report.resources) {
        display_resource_usage(&report.resources);
    }

    // Service health (if any services)
    if !report.services.is_empty() {
        display_service_health(&report.services);
    }

    if report.is_running {
        if let Some(ports_summary) = format_ports_summary(&report.services) {
            vm_println!("\n🔌 Ports: {}", ports_summary);
        }
    }

    // Connection hint
    if report.is_running {
        vm_println!("\n💡 Connect: vm ssh");
    } else {
        vm_println!("\n💡 Start: vm up");
    }
}

/// Display basic stopped status for providers without enhanced status support
fn display_basic_stopped_status(vm_name: &str, provider_name: &str) {
    vm_println!("🖥️  {} ({})", vm_name, provider_name);
    vm_println!("   🔴 Stopped");
    vm_println!("   📦 Container not found");
    vm_println!("\n💡 Start: vm up");
}

/// Check if resource data is available and meaningful
fn has_resource_data(resources: &vm_provider::ResourceUsage) -> bool {
    resources.cpu_percent.is_some()
        || resources.memory_used_mb.is_some()
        || resources.disk_used_gb.is_some()
}

/// Display resource usage information
fn display_resource_usage(resources: &vm_provider::ResourceUsage) {
    vm_println!("");

    // CPU usage
    if let Some(cpu) = resources.cpu_percent {
        let cpu_icon = if cpu > 80.0 {
            "🔥"
        } else if cpu > 50.0 {
            "⚡"
        } else {
            "💚"
        };
        vm_println!("   {} CPU:    {:.1}%", cpu_icon, cpu);
    }

    // Memory usage
    if let Some(used) = resources.memory_used_mb {
        let memory_text = if let Some(limit) = resources.memory_limit_mb {
            let usage_pct = (used as f64 / limit as f64) * 100.0;
            let mem_icon = if usage_pct > 90.0 {
                "🔥"
            } else if usage_pct > 70.0 {
                "⚡"
            } else {
                "💚"
            };
            let used_display = format_memory_mb(used);
            let limit_display = format_memory_mb(limit);
            format!("   {mem_icon} Memory: {used_display} / {limit_display} ({usage_pct:.0}%)")
        } else {
            let used_display = format_memory_mb(used);
            format!("   💚 Memory: {used_display}")
        };
        vm_println!("{}", memory_text);
    }

    // Disk usage
    if let Some(used) = resources.disk_used_gb {
        let disk_text = if let Some(total) = resources.disk_total_gb {
            let usage_pct = (used / total) * 100.0;
            let disk_icon = if usage_pct > 90.0 {
                "🔥"
            } else if usage_pct > 80.0 {
                "⚡"
            } else {
                "💚"
            };
            format!("   {disk_icon} Disk:   {used:.1}GB / {total:.1}GB ({usage_pct:.0}%)")
        } else {
            format!("   💚 Disk:   {used:.1}GB")
        };
        vm_println!("{}", disk_text);
    }
}

/// Display service health information
fn display_service_health(services: &[vm_provider::ServiceStatus]) {
    vm_println!("");

    for service in services {
        let health_icon = if service.is_running { "🟢" } else { "🔴" };
        let port_info = match (service.port, service.host_port) {
            (Some(container_port), Some(host_port)) if container_port != host_port => {
                format!(" ({host_port}→{container_port})")
            }
            (Some(port), _) => format!(" ({port})"),
            _ => String::new(),
        };

        let password = get_password_for_service(&service.name);

        let service_line = if let Some(metrics) = &service.metrics {
            format!(
                "   {} {}{} • {}",
                health_icon, service.name, port_info, metrics
            )
        } else if let Some(error) = &service.error {
            format!(
                "   {} {}{} • {}",
                health_icon, service.name, port_info, error
            )
        } else {
            format!("   {} {}{}", health_icon, service.name, port_info)
        };

        vm_println!("{}", service_line);
        if let Some(password) = password {
            vm_println!("     └── 🔑 {}", password);
        }
    }
}

fn format_ports_summary(services: &[vm_provider::ServiceStatus]) -> Option<String> {
    use std::collections::BTreeSet;

    let mut ports = BTreeSet::new();
    for service in services {
        if let Some(container_port) = service.port {
            let display = match service.host_port {
                Some(host_port) if host_port != container_port => {
                    format!("localhost:{host_port}->{container_port}")
                }
                Some(host_port) => format!("localhost:{host_port}"),
                None => container_port.to_string(),
            };
            ports.insert(display);
        }
    }

    if ports.is_empty() {
        None
    } else {
        Some(ports.into_iter().collect::<Vec<_>>().join(", "))
    }
}

/// Get the password for a service from the secrets store.
fn get_password_for_service(service_name: &str) -> Option<String> {
    if let Ok(secrets_dir) = vm_core::user_paths::secrets_dir() {
        let secret_file = secrets_dir.join(format!("{}.env", service_name));
        if secret_file.exists() {
            if let Ok(password) = std::fs::read_to_string(secret_file) {
                return Some(password.trim().to_string());
            }
        }
    }
    None
}

/// Format memory size in MB to human-readable format
fn format_memory_mb(mb: u64) -> String {
    if mb >= 1024 {
        format!("{:.1}GB", mb as f64 / 1024.0)
    } else {
        format!("{mb}MB")
    }
}
