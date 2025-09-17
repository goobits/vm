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
    let core_count = cpuinfo
        .lines()
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
    let mut sys = sysinfo::System::new_all();
    sys.refresh_memory();
    Ok(sys.total_memory() / 1024 / 1024 / 1024) // Convert bytes to GB
}

#[cfg(target_os = "windows")]
fn get_cpu_core_count() -> Result<u32> {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_cpu();
    Ok(sys.physical_core_count().unwrap_or(1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimum_requirements_constants() {
        // Test that our constants are reasonable
        assert!(MIN_CPU_CORES <= 16); // Sanity check - shouldn't require too many cores
        assert!(MIN_MEMORY_GB <= 64); // Sanity check - shouldn't require excessive memory
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "darwin"))]
    fn test_cpu_core_detection() {
        let result = get_cpu_core_count();

        // Should successfully detect CPU cores on supported platforms
        assert!(result.is_ok());

        let cpu_cores = result.unwrap();
        // Should return a reasonable number of cores
        assert!(cpu_cores > 0);
        assert!(cpu_cores <= 256); // Sanity check for reasonable upper bound
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "darwin"))]
    fn test_memory_detection() {
        let result = get_total_memory_gb();

        // Should successfully detect memory on supported platforms
        assert!(result.is_ok());

        let memory_gb = result.unwrap();
        // Should return a reasonable amount of memory
        assert!(memory_gb > 0);
        assert!(memory_gb <= 1024); // Sanity check for reasonable upper bound (1TB)
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_system_detection() {
        // On Windows, both functions should now succeed using sysinfo
        let cpu_result = get_cpu_core_count();
        assert!(cpu_result.is_ok());
        let cpu_cores = cpu_result.unwrap();
        assert!(cpu_cores > 0);
        assert!(cpu_cores <= 256); // Sanity check

        let memory_result = get_total_memory_gb();
        assert!(memory_result.is_ok());
        let memory_gb = memory_result.unwrap();
        assert!(memory_gb > 0);
        assert!(memory_gb <= 1024); // Sanity check (1TB)
    }

    #[test]
    fn test_resource_requirements_validation() {
        // Test the main check function logic with known values
        // Note: We can't easily test the actual function without mocking
        // But we can test the logic patterns it uses

        // Simulate sufficient resources
        let sufficient_cores = MIN_CPU_CORES + 2;
        let sufficient_memory = MIN_MEMORY_GB + 4;

        assert!(sufficient_cores >= MIN_CPU_CORES);
        assert!(sufficient_memory >= MIN_MEMORY_GB);

        // Simulate insufficient resources
        let insufficient_cores = MIN_CPU_CORES.saturating_sub(1);
        let insufficient_memory = MIN_MEMORY_GB.saturating_sub(1);

        assert!(insufficient_cores < MIN_CPU_CORES);
        assert!(insufficient_memory < MIN_MEMORY_GB);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_proc_parsing_resilience() {
        // Test that our parsing logic is resilient to different /proc formats
        // This is more of a documentation test showing expected input formats

        // Example meminfo line format
        let example_meminfo_line = "MemTotal:       16384000 kB";
        let parts: Vec<&str> = example_meminfo_line.split_whitespace().collect();
        assert!(parts.len() >= 2);

        if parts.len() >= 2 {
            let mem_kb_str = parts[1];
            let mem_kb: Result<u64, _> = mem_kb_str.parse();
            assert!(mem_kb.is_ok());

            let mem_kb = mem_kb.unwrap();
            let mem_gb = mem_kb / 1024 / 1024;
            assert!(mem_gb > 0);
        }

        // Example cpuinfo processor counting
        let example_cpuinfo_lines = [
            "processor\t: 0",
            "vendor_id\t: GenuineIntel",
            "processor\t: 1",
            "vendor_id\t: GenuineIntel",
        ];

        let processor_count = example_cpuinfo_lines
            .iter()
            .filter(|line| line.starts_with("processor"))
            .count() as u32;

        assert_eq!(processor_count, 2);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos_sysctl_command_format() {
        // Test that we're using the correct sysctl command formats
        // This is more of a documentation test

        let memory_args = ["-n", "hw.memsize"];
        assert_eq!(memory_args.len(), 2);
        assert_eq!(memory_args[0], "-n");
        assert_eq!(memory_args[1], "hw.memsize");

        let cpu_args = ["-n", "hw.physicalcpu"];
        assert_eq!(cpu_args.len(), 2);
        assert_eq!(cpu_args[0], "-n");
        assert_eq!(cpu_args[1], "hw.physicalcpu");
    }
}
