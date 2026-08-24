use crate::error::{Result, VmError};
use vm_platform::platform;

const MIN_CPU_CORES: u32 = 2;
const MIN_MEMORY_GB: u64 = 4;

/// Checks if the system meets the minimum resource requirements.
pub fn check_system_resources() -> Result<()> {
    validate_system_resources(platform::cpu_core_count()?, platform::total_memory_gb()?)
}

fn validate_system_resources(cpu_cores: u32, total_memory_gb: u64) -> Result<()> {
    if cpu_cores < MIN_CPU_CORES {
        return Err(VmError::Internal(format!(
            "System has only {cpu_cores} physical CPU cores; at least {MIN_CPU_CORES} are required"
        )));
    }

    if total_memory_gb < MIN_MEMORY_GB {
        return Err(VmError::Internal(format!(
            "System has only {total_memory_gb} GB of memory; at least {MIN_MEMORY_GB} GB are required"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_system_resources, MIN_CPU_CORES, MIN_MEMORY_GB};

    #[test]
    fn resource_requirements_have_clear_thresholds() {
        assert!(validate_system_resources(MIN_CPU_CORES, MIN_MEMORY_GB).is_ok());
        assert!(validate_system_resources(MIN_CPU_CORES - 1, MIN_MEMORY_GB).is_err());
        assert!(validate_system_resources(MIN_CPU_CORES, MIN_MEMORY_GB - 1).is_err());
    }
}
