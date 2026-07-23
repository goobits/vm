//! VM status reporting and display
//!
//! This module provides comprehensive status reporting for VMs with
//! resource usage, service health, and state information.

use tracing::debug;

use crate::error::VmResult;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::vm_println;
use vm_provider::{Provider, ResourceUsage, RuntimeDiagnostics, VmStatusReport};

/// Handle VM status check with enhanced dashboard
pub fn handle_status(
    provider: Box<dyn Provider>,
    container: Option<&str>,
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
        "Status check: vm_name='{}', provider='{}'",
        vm_name,
        provider.name()
    );

    let report = provider.status(container)?;
    display_status_dashboard(&report);
    Ok(())
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

    if let Some(runtime) = &report.runtime {
        display_runtime_diagnostics(runtime, &report.resources);
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
        vm_println!("\n💡 Start: vm start");
    }
}

fn display_runtime_diagnostics(runtime: &RuntimeDiagnostics, resources: &ResourceUsage) {
    vm_println!("\nRuntime evidence");

    if let Some(path) = &runtime.generated_config {
        let state = if runtime.generated_config_exists {
            "present"
        } else {
            "missing"
        };
        vm_println!("   Generated config: {} ({})", path.display(), state);
    }

    if let Some(bytes) = runtime.writable_layer_bytes {
        vm_println!("   Writable layer: {}", format_bytes(bytes));
    }
    if let Some(bytes) = runtime.root_filesystem_bytes {
        vm_println!("   Root filesystem: {}", format_bytes(bytes));
    }

    if let Some(peak) = runtime.memory_peak_bytes {
        let limit = resources
            .memory_limit_mb
            .map(|megabytes| megabytes * 1024 * 1024);
        vm_println!(
            "   Memory peak: {}",
            format_usage_with_headroom(peak, limit)
        );
    }

    if runtime.pids_current.is_some() || runtime.pids_peak.is_some() {
        let current = runtime
            .pids_current
            .map_or_else(|| "unknown".to_string(), |value| value.to_string());
        let peak = runtime
            .pids_peak
            .map_or_else(|| "unknown".to_string(), |value| value.to_string());
        let limit = runtime
            .pids_limit
            .map_or_else(|| "unlimited".to_string(), |value| value.to_string());
        let headroom = runtime.pids_limit.and_then(|limit| {
            runtime
                .pids_peak
                .filter(|peak| *peak <= limit)
                .map(|peak| format!(", {:.0}% peak headroom", percent_remaining(peak, limit)))
        });
        vm_println!(
            "   PIDs: current {}, peak {}, limit {}{}",
            current,
            peak,
            limit,
            headroom.unwrap_or_default()
        );
    }

    if !runtime.mounts.is_empty() {
        vm_println!("   Storage (volume usage is separate from writable layer):");
        for mount in &runtime.mounts {
            let name = mount
                .name
                .as_deref()
                .map(|name| format!(", {name}"))
                .unwrap_or_default();
            let usage = match (mount.used_bytes, mount.capacity_bytes) {
                (Some(used), Some(capacity)) => format!(
                    ", {} / {} ({:.0}%)",
                    format_bytes(used),
                    format_bytes(capacity),
                    percent_used(used, capacity)
                ),
                (Some(used), None) => format!(", {} used", format_bytes(used)),
                _ => String::new(),
            };
            let options = mount
                .options
                .as_deref()
                .map(|options| format!(", {options}"))
                .unwrap_or_default();
            vm_println!(
                "      {}: {}{}{}{}",
                mount.target,
                mount.storage_type,
                name,
                usage,
                options
            );
        }
    }

    if let Some(driver) = &runtime.logging_driver {
        let options = runtime
            .logging_options
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if options.is_empty() {
            String::new()
        } else {
            format!(" ({options})")
        };
        vm_println!("   Logs: {}{}", driver, suffix);
    }

    if runtime.restart_policy.is_some() || runtime.stop_timeout_seconds.is_some() {
        let restart = runtime.restart_policy.as_deref().unwrap_or("unknown");
        let timeout = runtime
            .stop_timeout_seconds
            .map_or_else(|| "unknown".to_string(), |seconds| format!("{seconds}s"));
        vm_println!(
            "   Lifecycle: restart {}, stop timeout {}",
            restart,
            timeout
        );
    }
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

/// Format memory size in MB to human-readable format
fn format_memory_mb(mb: u64) -> String {
    if mb >= 1024 {
        format!("{:.1}GB", mb as f64 / 1024.0)
    } else {
        format!("{mb}MB")
    }
}

fn format_usage_with_headroom(used: u64, limit: Option<u64>) -> String {
    match limit {
        Some(limit) if used <= limit => format!(
            "{} / {} ({:.0}% headroom)",
            format_bytes(used),
            format_bytes(limit),
            percent_remaining(used, limit)
        ),
        Some(limit) => format!("{} / {}", format_bytes(used), format_bytes(limit)),
        None => format!("{} / unlimited", format_bytes(used)),
    }
}

fn percent_used(used: u64, capacity: u64) -> f64 {
    if capacity == 0 {
        0.0
    } else {
        used as f64 / capacity as f64 * 100.0
    }
}

fn percent_remaining(used: u64, limit: u64) -> f64 {
    100.0 - percent_used(used, limit)
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

#[cfg(test)]
mod tests {
    use super::{format_bytes, format_usage_with_headroom};

    #[test]
    fn formats_binary_storage_units() {
        assert_eq!(format_bytes(56_245_325_824), "52.38 GiB");
        assert_eq!(format_bytes(1_048_576), "1.0 MiB");
    }

    #[test]
    fn reports_limit_headroom_from_peak_usage() {
        assert_eq!(
            format_usage_with_headroom(7 * 1024, Some(10 * 1024)),
            "7.0 KiB / 10.0 KiB (30% headroom)"
        );
        assert_eq!(
            format_usage_with_headroom(1024, None),
            "1.0 KiB / unlimited"
        );
    }
}
