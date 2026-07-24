//! Cleanup operation for pruning orphaned resources.
//!
//! This command cleans up unused Docker resources:
//! - VM-managed disposable volumes
//! - Stopped temp containers
//! - Old log files
//! - Dangling images
//! - Build cache

use crate::error::{VmError, VmResult};
use std::process::Command as StdCommand;
use std::time::{Duration, SystemTime};
use tracing::debug;
use vm_core::{vm_println, vm_progress, vm_success};

/// Results from cleanup operations
#[derive(Default)]
pub struct CleanupResults {
    pub volumes: u32,
    pub temp_containers: u32,
    pub log_files: u32,
    pub dangling_images: u32,
    pub build_cache_mb: u64,
}

/// Handle cleanup for `vm doctor --clean`
pub async fn handle_clean() -> VmResult<()> {
    let executable = detect_container_runtime();
    vm_progress!("Cleaning unused resources...");

    let results = CleanupResults {
        volumes: clean_dangling_volumes(&executable)?,
        temp_containers: clean_stopped_temp_containers(&executable)?,
        log_files: clean_old_logs(30)?,
        dangling_images: clean_dangling_images(&executable)?,
        build_cache_mb: clean_build_cache(&executable)?,
    };

    print_cleanup_summary(&results);
    Ok(())
}

const MANAGED_DISPOSABLE_VOLUME_FILTERS: [&str; 3] = [
    "dangling=true",
    "label=com.vm.managed=true",
    "label=com.vm.retention=disposable",
];

/// Clean dangling volumes that VM explicitly marked as disposable.
fn clean_dangling_volumes(executable: &str) -> VmResult<u32> {
    debug!("Cleaning VM-managed disposable volumes");

    let mut command = StdCommand::new(executable);
    command.args(["volume", "ls"]);
    for filter in MANAGED_DISPOSABLE_VOLUME_FILTERS {
        command.args(["--filter", filter]);
    }
    let output = command
        .arg("--quiet")
        .output()
        .map_err(|e| VmError::general(e, "Failed to list dangling volumes"))?;

    if !output.status.success() {
        return Ok(0);
    }

    let volumes: Vec<&str> = std::str::from_utf8(&output.stdout)
        .unwrap_or("")
        .lines()
        .filter(|s| !s.is_empty())
        .collect();

    if volumes.is_empty() {
        return Ok(0);
    }

    let mut removed = 0;
    for volume in volumes {
        let Ok(output) = StdCommand::new(executable)
            .args(["volume", "rm", volume])
            .output()
        else {
            continue;
        };
        if output.status.success() {
            removed += 1;
        }
    }
    if removed > 0 {
        vm_println!(
            "  Volumes: Removed {} managed disposable volume(s)",
            removed
        );
    }
    Ok(removed)
}

/// Clean stopped temp containers
fn clean_stopped_temp_containers(executable: &str) -> VmResult<u32> {
    debug!("Cleaning stopped temp containers");

    // Look for containers with vm-temp label that are stopped
    let output = StdCommand::new(executable)
        .args([
            "ps",
            "-a",
            "--filter",
            "name=vm-temp",
            "--filter",
            "status=exited",
            "--format",
            "{{.ID}}\t{{.Names}}",
        ])
        .output()
        .map_err(|e| VmError::general(e, "Failed to list stopped temp containers"))?;

    if !output.status.success() {
        return Ok(0);
    }

    let containers: Vec<(&str, &str)> = std::str::from_utf8(&output.stdout)
        .unwrap_or("")
        .lines()
        .filter(|s| !s.is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                Some((parts[0], parts[1]))
            } else {
                None
            }
        })
        .collect();

    if containers.is_empty() {
        return Ok(0);
    }

    let mut removed = 0;
    for (id, _) in containers {
        if StdCommand::new(executable)
            .args(["rm", id])
            .output()
            .is_ok_and(|output| output.status.success())
        {
            removed += 1;
        }
    }
    if removed > 0 {
        vm_println!("  Temp containers: Removed {removed} container(s)");
    }
    Ok(removed)
}

/// Clean old log files
fn clean_old_logs(days: u32) -> VmResult<u32> {
    debug!("Cleaning old log files (older than {} days)", days);

    let logs_dir = match vm_core::user_paths::user_data_dir() {
        Ok(dir) => dir.join("logs"),
        Err(_) => return Ok(0),
    };

    if !logs_dir.exists() {
        return Ok(0);
    }

    let cutoff = SystemTime::now() - Duration::from_secs(days as u64 * 86400);
    let mut count = 0u32;

    let entries = match std::fs::read_dir(&logs_dir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified = match metadata.modified() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if modified < cutoff && std::fs::remove_file(&path).is_ok() {
            count += 1;
        }
    }

    if count > 0 {
        vm_println!("  Logs: Removed {count} old log file(s)");
    }

    Ok(count)
}

/// Clean dangling Docker images
fn clean_dangling_images(executable: &str) -> VmResult<u32> {
    debug!("Cleaning dangling images");

    // Get count of dangling images
    let output = StdCommand::new(executable)
        .args(["image", "ls", "--filter", "dangling=true", "--quiet"])
        .output()
        .map_err(|e| VmError::general(e, "Failed to list dangling images"))?;

    if !output.status.success() {
        return Ok(0);
    }

    let count = std::str::from_utf8(&output.stdout)
        .unwrap_or("")
        .lines()
        .filter(|s| !s.is_empty())
        .count() as u32;

    if count == 0 {
        return Ok(0);
    }

    let removed = StdCommand::new(executable)
        .args(["image", "prune", "-f"])
        .output()
        .is_ok_and(|output| output.status.success());
    if removed {
        vm_println!("  Images: Removed {count} dangling image(s)");
        Ok(count)
    } else {
        Ok(0)
    }
}

/// Clean Docker build cache
fn clean_build_cache(executable: &str) -> VmResult<u64> {
    debug!("Cleaning build cache");

    let df_output = StdCommand::new(executable)
        .args([
            "builder",
            "du",
            "--filter",
            "type=regular",
            "--format",
            "{{.Size}}",
        ])
        .output();

    let cache_mb = if let Ok(out) = df_output {
        if out.status.success() {
            let size_str = std::str::from_utf8(&out.stdout).unwrap_or("0");
            parse_size_to_mb(size_str.trim())
        } else {
            0
        }
    } else {
        0
    };

    if cache_mb == 0 {
        return Ok(0);
    }

    let reclaimed = StdCommand::new(executable)
        .args(["builder", "prune", "-f"])
        .output()
        .is_ok_and(|output| output.status.success());
    if reclaimed {
        vm_println!("  Build cache: Reclaimed ~{cache_mb} MB");
        Ok(cache_mb)
    } else {
        Ok(0)
    }
}

/// Parse Docker size string to MB
fn parse_size_to_mb(size_str: &str) -> u64 {
    let size_str = size_str.to_uppercase();

    if size_str.contains("GB") {
        let num: f64 = size_str.replace("GB", "").trim().parse().unwrap_or(0.0);
        (num * 1024.0) as u64
    } else if size_str.contains("MB") {
        size_str.replace("MB", "").trim().parse().unwrap_or(0)
    } else if size_str.contains("KB") {
        let num: u64 = size_str.replace("KB", "").trim().parse().unwrap_or(0);
        num / 1024
    } else {
        0
    }
}

fn detect_container_runtime() -> String {
    vm_config::AppConfig::load(None, None, None)
        .ok()
        .and_then(|config| {
            config
                .vm
                .provider
                .or(config.global.defaults.provider)
                .filter(|provider| matches!(provider.as_str(), "docker" | "podman"))
        })
        .unwrap_or_else(|| "docker".to_string())
}

/// Print cleanup summary
fn print_cleanup_summary(results: &CleanupResults) {
    let total =
        results.volumes + results.temp_containers + results.log_files + results.dangling_images;

    if total == 0 && results.build_cache_mb == 0 {
        vm_success!("Nothing to clean; system is already tidy");
    } else {
        vm_success!("Cleanup complete");
        vm_println!(
            "   Removed: {} volumes, {} containers, {} logs, {} images",
            results.volumes,
            results.temp_containers,
            results.log_files,
            results.dangling_images
        );
        if results.build_cache_mb > 0 {
            vm_println!(
                "   Reclaimed: ~{} MB of build cache",
                results.build_cache_mb
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MANAGED_DISPOSABLE_VOLUME_FILTERS;

    #[test]
    fn volume_cleanup_requires_vm_ownership_and_disposable_retention() {
        assert_eq!(
            MANAGED_DISPOSABLE_VOLUME_FILTERS,
            [
                "dangling=true",
                "label=com.vm.managed=true",
                "label=com.vm.retention=disposable",
            ]
        );
    }
}
