use tracing::{info, warn};
use vm_config::config::VmConfig;
use vm_core::error::Result;
use vm_platform::platform;

use super::provider::TartProvider;
use crate::{resource_limits::ResolvedResources, VmError};

impl TartProvider {
    pub(super) fn apply_runtime_config(&self, instance: &str, config: &VmConfig) -> Result<()> {
        let resources = Self::resolved_tart_resources(config)?;
        if let Some(count) = resources.cpus {
            info!("Setting CPU count to {}", count);
            self.tart_expr(&["set", instance, "--cpu", &count.to_string()])
                .run()
                .map_err(|error| VmError::Provider(format!("Failed to set CPU: {error}")))?;
        }
        if let Some(memory) = resources.memory_mb {
            info!("Setting memory to {}MB", memory);
            self.tart_expr(&["set", instance, "--memory", &memory.to_string()])
                .run()
                .map_err(|error| VmError::Provider(format!("Failed to set memory: {error}")))?;
        }
        Ok(())
    }

    fn adjust_cpu_count(requested: u32) -> u32 {
        let available = platform::cpu_core_count().unwrap_or(2);
        if requested > available {
            (available / 2).max(1).min(available)
        } else {
            requested
        }
    }

    fn adjust_memory_mb(requested: u32) -> u32 {
        let safe_gb = platform::total_memory_gb()
            .unwrap_or(4)
            .saturating_sub(2)
            .max(1);
        if u64::from(requested) / 1024 > safe_gb {
            (safe_gb * 1024) as u32
        } else {
            requested
        }
    }

    pub(super) fn resolved_tart_resources(config: &VmConfig) -> Result<ResolvedResources> {
        let requested = ResolvedResources::resolve(config)?;
        let cpus = requested.cpus.map(|value| {
            let adjusted = Self::adjust_cpu_count(value);
            if adjusted != value { warn!("Tart requested {value} CPUs, but the host can safely apply {adjusted}; using {adjusted}"); }
            adjusted
        });
        let memory_mb = requested.memory_mb.map(|value| {
            let adjusted = Self::adjust_memory_mb(value);
            if adjusted != value { warn!("Tart requested {value} MB RAM, but the host can safely apply {adjusted} MB; using {adjusted} MB"); }
            adjusted
        });
        let host_cpus = platform::cpu_core_count().unwrap_or(2);
        let host_memory_mb = platform::total_memory_gb()
            .unwrap_or(4)
            .saturating_mul(1024);
        if Self::uses_most_of_host(cpus, memory_mb, host_cpus, host_memory_mb) {
            warn!("Tart is configured for at least 75% of this host; Docker Desktop or another VM may oversubscribe macOS");
        }
        Ok(ResolvedResources { memory_mb, cpus })
    }

    pub(super) fn uses_most_of_host(
        cpus: Option<u32>,
        memory: Option<u32>,
        host_cpus: u32,
        host_memory: u64,
    ) -> bool {
        cpus.is_some_and(|value| value.saturating_mul(4) >= host_cpus.saturating_mul(3))
            || memory.is_some_and(|value| {
                u64::from(value).saturating_mul(4) >= host_memory.saturating_mul(3)
            })
    }
}
