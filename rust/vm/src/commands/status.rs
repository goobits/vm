use serde::Serialize;
use vm_core::vm_println;
use vm_provider::{MountUsage, ResourceUsage, RuntimeDiagnostics, VmStatusReport};

use crate::error::{VmError, VmResult};
use vm_provider::InstanceProvider;

#[derive(Serialize)]
pub(super) struct StatusView {
    pub name: String,
    pub provider: String,
    pub state: String,
    pub is_running: bool,
    pub uptime: Option<String>,
    pub resources: ResourceView,
    pub services: Vec<ServiceView>,
    pub configuration: &'static str,
}

#[derive(Serialize)]
pub(super) struct StatusTarget {
    pub target: String,
    pub provider: String,
    pub ok: bool,
    pub status: Option<StatusView>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub(super) struct StatusOutput {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub targets: Vec<StatusTarget>,
}

impl StatusOutput {
    pub fn from_targets(targets: Vec<StatusTarget>) -> Self {
        let total = targets.len();
        let succeeded = targets.iter().filter(|target| target.ok).count();
        Self {
            total,
            succeeded,
            failed: total - succeeded,
            targets,
        }
    }
}

impl StatusTarget {
    pub fn success(target: String, provider: String, status: StatusView) -> Self {
        Self {
            target,
            provider,
            ok: true,
            status: Some(status),
            error: None,
        }
    }

    pub fn failure(target: String, provider: String, error: &VmError) -> Self {
        Self {
            target,
            provider,
            ok: false,
            status: None,
            error: Some(error.to_string()),
        }
    }
}

#[derive(Serialize)]
pub(super) struct ResourceView {
    cpu_percent: Option<f64>,
    memory_used_mb: Option<u64>,
    memory_limit_mb: Option<u64>,
    disk_used_gb: Option<f64>,
    disk_total_gb: Option<f64>,
}

#[derive(Serialize)]
pub(super) struct ServiceView {
    name: String,
    is_running: bool,
    port: Option<u16>,
    host_port: Option<u16>,
}

pub(super) fn collect(provider: &dyn InstanceProvider, target: &str) -> VmResult<StatusView> {
    let report = provider.status(Some(target)).map_err(VmError::from)?;
    let configuration = if !provider.supports_runtime_drift_detection() {
        "unknown"
    } else if provider
        .runtime_drift(target)
        .map_err(VmError::from)?
        .is_some()
    {
        "drifted"
    } else {
        "current"
    };
    Ok(StatusView::from_report(report, configuration))
}

impl StatusView {
    fn from_report(report: VmStatusReport, configuration: &'static str) -> Self {
        Self {
            name: report.name,
            provider: report.provider,
            state: report.state.to_string(),
            is_running: report.is_running,
            uptime: report.uptime,
            resources: ResourceView {
                cpu_percent: report.resources.cpu_percent,
                memory_used_mb: report.resources.memory_used_mb,
                memory_limit_mb: report.resources.memory_limit_mb,
                disk_used_gb: report.resources.disk_used_gb,
                disk_total_gb: report.resources.disk_total_gb,
            },
            services: report
                .services
                .into_iter()
                .map(|service| ServiceView {
                    name: service.name,
                    is_running: service.is_running,
                    port: service.port,
                    host_port: service.host_port,
                })
                .collect(),
            configuration,
        }
    }
}

pub(super) fn display(report: &VmStatusReport) {
    vm_println!("{}\t{}\t{}", report.name, report.provider, report.state);

    if let Some(uptime) = &report.uptime {
        vm_println!("Uptime: {uptime}");
    }
    if report.is_running {
        display_resources(&report.resources);
    }
    for service in &report.services {
        let state = if service.is_running {
            "running"
        } else {
            "stopped"
        };
        vm_println!("Service {}: {}", service.name, state);
    }
    if let Some(runtime) = &report.runtime {
        display_runtime(runtime, &report.resources);
    }
}

fn display_resources(resources: &ResourceUsage) {
    if let Some(cpu) = resources.cpu_percent {
        vm_println!("CPU: {cpu:.1}%");
    }
    if let Some(used) = resources.memory_used_mb {
        match resources.memory_limit_mb {
            Some(limit) => vm_println!(
                "Memory: {} / {}",
                format_bytes(used * 1024 * 1024),
                format_bytes(limit * 1024 * 1024)
            ),
            None => vm_println!("Memory: {} / unlimited", format_bytes(used * 1024 * 1024)),
        }
    }
}

fn display_runtime(runtime: &RuntimeDiagnostics, resources: &ResourceUsage) {
    vm_println!("\nRuntime evidence:");
    if let Some(path) = &runtime.generated_config {
        let state = if runtime.generated_config_exists {
            "present"
        } else {
            "missing"
        };
        vm_println!("  Generated config: {} ({state})", path.display());
    }
    if let Some(bytes) = runtime.writable_layer_bytes {
        vm_println!("  Writable layer: {}", format_bytes(bytes));
    }
    if let Some(bytes) = runtime.root_filesystem_bytes {
        vm_println!("  Root filesystem: {}", format_bytes(bytes));
    }
    if let Some(peak) = runtime.memory_peak_bytes {
        let limit = resources
            .memory_limit_mb
            .map(|megabytes| megabytes * 1024 * 1024);
        vm_println!("  Memory peak: {}", format_headroom(peak, limit));
    }
    if runtime.pids_current.is_some() || runtime.pids_peak.is_some() {
        vm_println!(
            "  PIDs: current {}, peak {}, limit {}{}",
            optional_number(runtime.pids_current),
            optional_number(runtime.pids_peak),
            runtime
                .pids_limit
                .map_or_else(|| "unlimited".to_string(), |value| value.to_string()),
            runtime
                .pids_limit
                .zip(runtime.pids_peak)
                .filter(|(limit, peak)| peak <= limit)
                .map_or_else(String::new, |(limit, peak)| {
                    format!(", {:.0}% peak headroom", percent_remaining(peak, limit))
                })
        );
    }

    if !runtime.mounts.is_empty() {
        vm_println!("  Storage (volume usage is separate from writable layer):");
        for mount in &runtime.mounts {
            vm_println!("    {}", format_mount(mount));
        }
    }
    if let Some(driver) = &runtime.logging_driver {
        let options = runtime
            .logging_options
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ");
        let options = if options.is_empty() {
            String::new()
        } else {
            format!(" ({options})")
        };
        vm_println!("  Logs: {driver}{options}");
    }
    if runtime.restart_policy.is_some() || runtime.stop_timeout_seconds.is_some() {
        vm_println!(
            "  Lifecycle: restart {}, stop timeout {}",
            runtime.restart_policy.as_deref().unwrap_or("unknown"),
            runtime
                .stop_timeout_seconds
                .map_or_else(|| "unknown".to_string(), |seconds| format!("{seconds}s"))
        );
    }
}

fn format_mount(mount: &MountUsage) -> String {
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
    format!(
        "{}: {}{}{}{}",
        mount.target, mount.storage_type, name, usage, options
    )
}

fn optional_number(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

fn format_headroom(used: u64, limit: Option<u64>) -> String {
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
    use super::{format_bytes, format_headroom, StatusOutput, StatusTarget, StatusView};
    use vm_provider::{RuntimeDiagnostics, ServiceStatus, VmStatusReport};

    #[test]
    fn formats_binary_storage_units() {
        assert_eq!(format_bytes(56_245_325_824), "52.38 GiB");
        assert_eq!(format_bytes(1_048_576), "1.0 MiB");
    }

    #[test]
    fn reports_limit_headroom_from_peak_usage() {
        assert_eq!(
            format_headroom(7 * 1024, Some(10 * 1024)),
            "7.0 KiB / 10.0 KiB (30% headroom)"
        );
        assert_eq!(format_headroom(1024, None), "1.0 KiB / unlimited");
    }

    #[test]
    fn json_status_omits_untrusted_runtime_paths_and_service_details() {
        let report = VmStatusReport {
            name: "dev".into(),
            provider: "docker".into(),
            runtime: Some(RuntimeDiagnostics {
                generated_config: Some("/private/secret-runtime-path".into()),
                logging_options: vec![("token".into(), "secret-log-option".into())],
                ..Default::default()
            }),
            services: vec![ServiceStatus {
                name: "db".into(),
                is_running: true,
                port: Some(5432),
                host_port: None,
                metrics: Some("secret-service-metric".into()),
                error: None,
            }],
            ..Default::default()
        };
        let view = StatusView::from_report(report, "drifted");
        let json = serde_json::to_string(&view).unwrap();
        assert!(json.contains("\"configuration\":\"drifted\""));
        assert!(!json.contains("secret-runtime-path"));
        assert!(!json.contains("secret-log-option"));
        assert!(!json.contains("secret-service-metric"));
    }

    #[test]
    fn status_output_counts_per_target_results() {
        let output = StatusOutput::from_targets(vec![
            StatusTarget::success(
                "dev".into(),
                "docker".into(),
                StatusView::from_report(VmStatusReport::default(), "unknown"),
            ),
            StatusTarget::failure(
                "test".into(),
                "docker".into(),
                &crate::error::VmError::validation("missing", None::<String>),
            ),
        ]);
        assert_eq!((output.total, output.succeeded, output.failed), (2, 1, 1));
    }
}
