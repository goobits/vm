//! Cleanup operation for pruning orphaned resources.
//!
//! This command cleans up unused Docker resources:
//! - VM-managed disposable volumes
//! - Stopped VM temporary containers
//! - Old log files
//! - VM-managed dangling images

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
}

/// Handle cleanup for `vm doctor --clean`
pub async fn handle_clean() -> VmResult<()> {
    let executable = crate::utils::configured_container_runtime();
    vm_progress!("Cleaning unused resources...");

    let results = CleanupResults {
        volumes: clean_dangling_volumes(&executable)?,
        temp_containers: clean_stopped_temp_containers(&executable)?,
        log_files: clean_old_logs(30)?,
        dangling_images: clean_dangling_images(&executable)?,
    };

    print_cleanup_summary(&results);
    Ok(())
}

const MANAGED_DISPOSABLE_VOLUME_FILTERS: [&str; 3] = [
    "dangling=true",
    "label=com.vm.managed=true",
    "label=com.vm.retention=disposable",
];

const STOPPED_TEMP_CONTAINER_FILTERS: [&str; 3] = [
    "label=com.vm.managed=true",
    "label=com.vm.temporary=true",
    "status=exited",
];

const MANAGED_DANGLING_IMAGE_FILTERS: [&str; 2] = ["dangling=true", "label=com.vm.managed=true"];

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

    let mut command = StdCommand::new(executable);
    command.args(["ps", "-a"]);
    for filter in STOPPED_TEMP_CONTAINER_FILTERS {
        command.args(["--filter", filter]);
    }
    let output = command
        .args(["--format", "{{.ID}}\t{{.Names}}"])
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

/// Clean dangling images that VM explicitly marked as managed.
fn clean_dangling_images(executable: &str) -> VmResult<u32> {
    debug!("Cleaning VM-managed dangling images");

    let mut command = StdCommand::new(executable);
    command.args(["image", "ls"]);
    for filter in MANAGED_DANGLING_IMAGE_FILTERS {
        command.args(["--filter", filter]);
    }
    let output = command
        .arg("--quiet")
        .output()
        .map_err(|e| VmError::general(e, "Failed to list VM-managed dangling images"))?;

    if !output.status.success() {
        return Ok(0);
    }

    let images = std::str::from_utf8(&output.stdout)
        .unwrap_or("")
        .lines()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    if images.is_empty() {
        return Ok(0);
    }

    let mut removed = 0;
    for image in images {
        if StdCommand::new(executable)
            .args(["image", "rm", image])
            .output()
            .is_ok_and(|output| output.status.success())
        {
            removed += 1;
        }
    }
    if removed > 0 {
        vm_println!("  Images: Removed {removed} VM-managed dangling image(s)");
    }
    Ok(removed)
}

/// Print cleanup summary
fn print_cleanup_summary(results: &CleanupResults) {
    let total =
        results.volumes + results.temp_containers + results.log_files + results.dangling_images;

    if total == 0 {
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
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MANAGED_DANGLING_IMAGE_FILTERS, MANAGED_DISPOSABLE_VOLUME_FILTERS,
        STOPPED_TEMP_CONTAINER_FILTERS,
    };

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

    #[test]
    fn container_and_image_cleanup_require_vm_ownership() {
        assert_eq!(
            STOPPED_TEMP_CONTAINER_FILTERS,
            [
                "label=com.vm.managed=true",
                "label=com.vm.temporary=true",
                "status=exited",
            ]
        );
        assert_eq!(
            MANAGED_DANGLING_IMAGE_FILTERS,
            ["dangling=true", "label=com.vm.managed=true"]
        );
    }
}
