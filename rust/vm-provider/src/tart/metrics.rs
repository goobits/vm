use serde::Deserialize;
use vm_core::error::Result;

use super::provider::TartProvider;
use crate::{ResourceUsage, ServiceStatus, VmError};

pub(super) struct CollectedMetrics {
    pub(super) resources: ResourceUsage,
    pub(super) services: Vec<ServiceStatus>,
    pub(super) uptime: Option<String>,
}

impl TartProvider {
    pub(super) fn collect_metrics(&self, instance: &str) -> Result<CollectedMetrics> {
        let output = self
            .tart_expr(&[
                "exec",
                instance,
                "sh",
                "-c",
                include_str!("scripts/collect_metrics.sh"),
            ])
            .stderr_capture()
            .read()
            .map_err(|error| VmError::Provider(format!("SSH command failed: {error}")))?;
        Self::parse_metrics_json(&output)
    }

    fn parse_metrics_json(raw: &str) -> Result<CollectedMetrics> {
        #[derive(Deserialize)]
        struct Payload {
            cpu_percent: Option<f64>,
            memory_used_mb: Option<u64>,
            memory_limit_mb: Option<u64>,
            disk_used_gb: Option<f64>,
            disk_total_gb: Option<f64>,
            uptime: Option<String>,
            services: Vec<ServiceEntry>,
        }
        #[derive(Deserialize)]
        struct ServiceEntry {
            name: String,
            is_running: bool,
        }
        let payload: Payload = serde_json::from_str(raw)
            .map_err(|error| VmError::Provider(format!("Failed to parse metrics JSON: {error}")))?;
        Ok(CollectedMetrics {
            resources: ResourceUsage {
                cpu_percent: payload.cpu_percent,
                memory_used_mb: payload.memory_used_mb,
                memory_limit_mb: payload.memory_limit_mb,
                disk_used_gb: payload.disk_used_gb,
                disk_total_gb: payload.disk_total_gb,
            },
            services: payload
                .services
                .into_iter()
                .map(|service| ServiceStatus {
                    name: service.name,
                    is_running: service.is_running,
                    port: None,
                    host_port: None,
                    metrics: None,
                    error: None,
                })
                .collect(),
            uptime: payload.uptime,
        })
    }
}
