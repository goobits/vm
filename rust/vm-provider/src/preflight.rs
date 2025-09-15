use anyhow::{bail, Result};
use sysinfo::System;

const MIN_CPU_CORES: u32 = 2;
const MIN_MEMORY_GB: u64 = 4;

/// Checks if the system meets the minimum resource requirements.
pub fn check_system_resources() -> Result<()> {
    let mut sys = System::new_all();
    sys.refresh_all();

    // Check CPU cores
    if let Some(cpu_cores) = System::physical_core_count() {
        if (cpu_cores as u32) < MIN_CPU_CORES {
            bail!(
                "System has only {} physical CPU cores. A minimum of {} is recommended.",
                cpu_cores,
                MIN_CPU_CORES
            );
        }
    }

    // Check Memory
    let total_memory_gb = sys.total_memory() / 1024 / 1024 / 1024;
    if total_memory_gb < MIN_MEMORY_GB {
        bail!(
            "System has only {} GB of memory. A minimum of {} GB is recommended.",
            total_memory_gb,
            MIN_MEMORY_GB
        );
    }

    Ok(())
}
