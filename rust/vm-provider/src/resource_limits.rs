use vm_config::config::{CpuLimit, MemoryLimit, VmConfig};
use vm_core::error::{Result, VmError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedResources {
    pub(crate) memory_mb: Option<u32>,
    pub(crate) cpus: Option<u32>,
}

impl ResolvedResources {
    pub(crate) fn resolve(config: &VmConfig) -> Result<Self> {
        let vm = config.vm.as_ref();
        let total_memory_mb = if matches!(
            vm.and_then(|settings| settings.memory.as_ref()),
            Some(MemoryLimit::Percentage(_))
        ) {
            vm_platform::platform::total_memory_gb()
                .map_err(|error| {
                    VmError::Internal(format!("Failed to resolve host memory: {error}"))
                })?
                .saturating_mul(1024)
        } else {
            0
        };
        let total_cpus = if matches!(
            vm.and_then(|settings| settings.cpus.as_ref()),
            Some(CpuLimit::Percentage(_))
        ) {
            vm_platform::platform::cpu_core_count().map_err(|error| {
                VmError::Internal(format!("Failed to resolve host CPU count: {error}"))
            })?
        } else {
            0
        };

        Ok(Self::resolve_for_host(config, total_memory_mb, total_cpus))
    }

    fn resolve_for_host(config: &VmConfig, total_memory_mb: u64, total_cpus: u32) -> Self {
        let vm = config.vm.as_ref();
        let memory_mb = vm
            .and_then(|settings| settings.memory.as_ref())
            .and_then(|limit| limit.resolve_percentage(total_memory_mb));
        let cpus = vm
            .and_then(|settings| settings.cpus.as_ref())
            .and_then(|limit| limit.resolve_percentage(total_cpus));

        Self { memory_mb, cpus }
    }
}

#[cfg(test)]
mod tests {
    use super::ResolvedResources;
    use vm_config::config::{CpuLimit, MemoryLimit, VmConfig, VmSettings};

    fn config(memory: MemoryLimit, cpus: CpuLimit) -> VmConfig {
        VmConfig {
            vm: Some(VmSettings {
                memory: Some(memory),
                cpus: Some(cpus),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn resolves_percentage_limits_for_all_providers() {
        let resolved = ResolvedResources::resolve_for_host(
            &config(MemoryLimit::Percentage(75), CpuLimit::Percentage(75)),
            16 * 1024,
            8,
        );

        assert_eq!(resolved.memory_mb, Some(12 * 1024));
        assert_eq!(resolved.cpus, Some(6));
    }

    #[test]
    fn preserves_fixed_and_unlimited_limits() {
        let fixed = ResolvedResources::resolve_for_host(
            &config(MemoryLimit::Limited(8192), CpuLimit::Limited(4)),
            16 * 1024,
            8,
        );
        let unlimited = ResolvedResources::resolve_for_host(
            &config(MemoryLimit::Unlimited, CpuLimit::Unlimited),
            16 * 1024,
            8,
        );

        assert_eq!(fixed.memory_mb, Some(8192));
        assert_eq!(fixed.cpus, Some(4));
        assert_eq!(unlimited.memory_mb, None);
        assert_eq!(unlimited.cpus, None);
    }
}
