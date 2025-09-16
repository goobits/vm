use anyhow::{bail, Result};

const MIN_CPU_CORES: u32 = 2;
const MIN_MEMORY_GB: u64 = 4;

/// Checks if the system meets the minimum resource requirements.
pub fn check_system_resources() -> Result<()> {
    // Check CPU cores
    let cpu_cores = get_cpu_core_count()?;
    if cpu_cores < MIN_CPU_CORES {
        bail!(
            "System has only {} physical CPU cores. A minimum of {} is recommended.",
            cpu_cores,
            MIN_CPU_CORES
        );
    }

    // Check Memory
    let total_memory_gb = get_total_memory_gb()?;
    if total_memory_gb < MIN_MEMORY_GB {
        bail!(
            "System has only {} GB of memory. A minimum of {} GB is recommended.",
            total_memory_gb,
            MIN_MEMORY_GB
        );
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn get_total_memory_gb() -> Result<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo")?;
    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let mem_kb: u64 = parts[1].parse()?;
                return Ok(mem_kb / 1024 / 1024); // Convert KB to GB
            }
        }
    }
    bail!("Could not parse memory information from /proc/meminfo");
}

#[cfg(target_os = "linux")]
fn get_cpu_core_count() -> Result<u32> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo")?;
    let core_count = cpuinfo.lines()
        .filter(|line| line.starts_with("processor"))
        .count() as u32;
    Ok(core_count)
}

#[cfg(target_os = "macos")]
fn get_total_memory_gb() -> Result<u64> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()?;

    if !output.status.success() {
        bail!("Failed to get memory size via sysctl");
    }

    let mem_bytes: u64 = String::from_utf8(output.stdout)?.trim().parse()?;
    Ok(mem_bytes / 1024 / 1024 / 1024) // Convert bytes to GB
}

#[cfg(target_os = "macos")]
fn get_cpu_core_count() -> Result<u32> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", "hw.physicalcpu"])
        .output()?;

    if !output.status.success() {
        bail!("Failed to get CPU count via sysctl");
    }

    let cpu_count: u32 = String::from_utf8(output.stdout)?.trim().parse()?;
    Ok(cpu_count)
}

#[cfg(target_os = "windows")]
fn get_total_memory_gb() -> Result<u64> {
    // For Windows, we'd use WMI or registry queries
    // For now, return a conservative estimate
    bail!("Windows system checking not implemented yet");
}

#[cfg(target_os = "windows")]
fn get_cpu_core_count() -> Result<u32> {
    // For Windows, we'd use WMI or GetSystemInfo
    // For now, return a conservative estimate
    bail!("Windows system checking not implemented yet");
}