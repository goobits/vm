//! Cleanup operation for pruning orphaned resources.
//!
//! This command cleans up unused Docker resources:
//! - VM-managed disposable volumes
//! - Stopped VM temporary containers
//! - Old log files
//! - VM-managed dangling images

use crate::error::{VmError, VmResult};
use std::process::{Command as StdCommand, Output};
use std::time::{Duration, SystemTime};
use tracing::debug;
use vm_config::AppConfig;
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
    let provider = AppConfig::load(None, None, None)?.container_provider();
    let executable = provider.as_str();
    vm_progress!("Cleaning unused resources...");

    let results = CleanupResults {
        volumes: clean_dangling_volumes(executable)?,
        temp_containers: clean_stopped_temp_containers(executable)?,
        log_files: clean_old_logs(30)?,
        dangling_images: clean_dangling_images(executable)?,
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
    let output = command_output(command.arg("--quiet"), "list dangling volumes")?;

    let volumes: Vec<&str> = output_text(&output, "list dangling volumes")?
        .lines()
        .filter(|s| !s.is_empty())
        .collect();

    if volumes.is_empty() {
        return Ok(0);
    }

    let mut removed = 0;
    for volume in volumes {
        command_output(
            StdCommand::new(executable).args(["volume", "rm", volume]),
            &format!("remove managed volume '{volume}'"),
        )?;
        removed += 1;
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
    let output = command_output(
        command.args(["--format", "{{.ID}}\t{{.Names}}"]),
        "list stopped temporary containers",
    )?;

    let containers: Vec<(&str, &str)> = output_text(&output, "list stopped temporary containers")?
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
        command_output(
            StdCommand::new(executable).args(["rm", id]),
            &format!("remove managed temporary container '{id}'"),
        )?;
        removed += 1;
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
    let output = command_output(command.arg("--quiet"), "list VM-managed dangling images")?;

    let images = output_text(&output, "list VM-managed dangling images")?
        .lines()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    if images.is_empty() {
        return Ok(0);
    }

    let mut removed = 0;
    for image in images {
        command_output(
            StdCommand::new(executable).args(["image", "rm", image]),
            &format!("remove VM-managed dangling image '{image}'"),
        )?;
        removed += 1;
    }
    if removed > 0 {
        vm_println!("  Images: Removed {removed} VM-managed dangling image(s)");
    }
    Ok(removed)
}

fn command_output(command: &mut StdCommand, operation: &str) -> VmResult<Output> {
    let output = command
        .output()
        .map_err(|error| VmError::general(error, format!("Failed to {operation}")))?;
    if output.status.success() {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim();
    let message = if detail.is_empty() {
        format!("Could not {operation} ({})", output.status)
    } else {
        format!("Could not {operation}: {detail}")
    };
    Err(VmError::validation(message, None::<String>))
}

fn output_text<'a>(output: &'a Output, operation: &str) -> VmResult<&'a str> {
    std::str::from_utf8(&output.stdout).map_err(|error| {
        VmError::general(error, format!("Invalid output while trying to {operation}"))
    })
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
        command_output, MANAGED_DANGLING_IMAGE_FILTERS, MANAGED_DISPOSABLE_VOLUME_FILTERS,
        STOPPED_TEMP_CONTAINER_FILTERS,
    };
    use std::process::Command;

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

    #[cfg(unix)]
    #[test]
    fn cleanup_command_failures_are_not_reported_as_empty_results() {
        let error = command_output(
            Command::new("sh").args(["-c", "printf 'engine unavailable' >&2; exit 7"]),
            "list managed resources",
        )
        .unwrap_err();

        assert!(error.to_string().contains("engine unavailable"));
    }
}
