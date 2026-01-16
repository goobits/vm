//! Cleanup operation for pruning orphaned resources.
//!
//! This command cleans up unused Docker resources:
//! - Dangling volumes
//! - Stopped temp containers
//! - Old log files
//! - Dangling images
//! - Build cache

use crate::error::{VmError, VmResult};
use std::process::Command as StdCommand;
use std::time::{Duration, SystemTime};
use tracing::debug;
use vm_core::{vm_println, vm_success};

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
pub async fn handle_clean(dry_run: bool, verbose: bool) -> VmResult<()> {
    if dry_run {
        vm_println!("🔍 Dry run mode - showing what would be cleaned\n");
    } else {
        vm_println!("🧹 Cleaning up unused resources...\n");
    }

    let results = CleanupResults {
        volumes: clean_dangling_volumes(dry_run, verbose)?,
        temp_containers: clean_stopped_temp_containers(dry_run, verbose)?,
        log_files: clean_old_logs(dry_run, verbose, 30)?,
        dangling_images: clean_dangling_images(dry_run, verbose)?,
        build_cache_mb: clean_build_cache(dry_run, verbose)?,
    };

    // Print summary
    print_cleanup_summary(&results, dry_run);

    Ok(())
}

/// Clean dangling Docker volumes
fn clean_dangling_volumes(dry_run: bool, verbose: bool) -> VmResult<u32> {
    debug!("Cleaning dangling volumes");

    let output = StdCommand::new("docker")
        .args(["volume", "ls", "--filter", "dangling=true", "--quiet"])
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

    let count = volumes.len() as u32;

    if count == 0 {
        if verbose {
            vm_println!("  Volumes: No dangling volumes found");
        }
        return Ok(0);
    }

    if dry_run {
        vm_println!("  Volumes: Would remove {} dangling volume(s)", count);
        if verbose {
            for vol in &volumes {
                vm_println!("    - {}", vol);
            }
        }
    } else {
        for vol in &volumes {
            let result = StdCommand::new("docker")
                .args(["volume", "rm", vol])
                .output();

            if let Ok(out) = result {
                if out.status.success() && verbose {
                    vm_println!("    Removed volume: {}", vol);
                }
            }
        }
        vm_println!("  Volumes: Removed {} dangling volume(s)", count);
    }

    Ok(count)
}

/// Clean stopped temp containers
fn clean_stopped_temp_containers(dry_run: bool, verbose: bool) -> VmResult<u32> {
    debug!("Cleaning stopped temp containers");

    // Look for containers with vm-temp label that are stopped
    let output = StdCommand::new("docker")
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

    let count = containers.len() as u32;

    if count == 0 {
        if verbose {
            vm_println!("  Temp containers: No stopped temp containers found");
        }
        return Ok(0);
    }

    if dry_run {
        vm_println!(
            "  Temp containers: Would remove {} stopped container(s)",
            count
        );
        if verbose {
            for (id, name) in &containers {
                vm_println!("    - {} ({})", name, id);
            }
        }
    } else {
        for (id, name) in &containers {
            let result = StdCommand::new("docker").args(["rm", id]).output();

            if let Ok(out) = result {
                if out.status.success() && verbose {
                    vm_println!("    Removed container: {}", name);
                }
            }
        }
        vm_println!("  Temp containers: Removed {} container(s)", count);
    }

    Ok(count)
}

/// Clean old log files
fn clean_old_logs(dry_run: bool, verbose: bool, days: u32) -> VmResult<u32> {
    debug!("Cleaning old log files (older than {} days)", days);

    let logs_dir = match vm_core::user_paths::user_data_dir() {
        Ok(dir) => dir.join("logs"),
        Err(_) => return Ok(0),
    };

    if !logs_dir.exists() {
        if verbose {
            vm_println!("  Logs: No logs directory found");
        }
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

        if modified < cutoff {
            if dry_run {
                count += 1;
                if verbose {
                    vm_println!("    Would remove: {}", path.display());
                }
            } else if std::fs::remove_file(&path).is_ok() {
                count += 1;
                if verbose {
                    vm_println!("    Removed: {}", path.display());
                }
            }
        }
    }

    if count > 0 {
        if dry_run {
            vm_println!("  Logs: Would remove {} old log file(s)", count);
        } else {
            vm_println!("  Logs: Removed {} old log file(s)", count);
        }
    } else if verbose {
        vm_println!("  Logs: No old log files found");
    }

    Ok(count)
}

/// Clean dangling Docker images
fn clean_dangling_images(dry_run: bool, verbose: bool) -> VmResult<u32> {
    debug!("Cleaning dangling images");

    // Get count of dangling images
    let output = StdCommand::new("docker")
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
        if verbose {
            vm_println!("  Images: No dangling images found");
        }
        return Ok(0);
    }

    if dry_run {
        vm_println!("  Images: Would remove {} dangling image(s)", count);
    } else {
        let prune_output = StdCommand::new("docker")
            .args(["image", "prune", "-f"])
            .output();

        if let Ok(out) = prune_output {
            if out.status.success() {
                vm_println!("  Images: Removed {} dangling image(s)", count);
            }
        }
    }

    Ok(count)
}

/// Clean Docker build cache
fn clean_build_cache(dry_run: bool, verbose: bool) -> VmResult<u64> {
    debug!("Cleaning build cache");

    // Get build cache size
    let output = StdCommand::new("docker")
        .args(["system", "df", "--format", "{{.Size}}"])
        .output()
        .map_err(|e| VmError::general(e, "Failed to get Docker system info"))?;

    if !output.status.success() {
        return Ok(0);
    }

    // Parse build cache size (rough estimate from system df)
    let df_output = StdCommand::new("docker")
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
        if verbose {
            vm_println!("  Build cache: No reclaimable cache found");
        }
        return Ok(0);
    }

    if dry_run {
        vm_println!("  Build cache: Would reclaim ~{} MB", cache_mb);
    } else {
        let prune_output = StdCommand::new("docker")
            .args(["builder", "prune", "-f"])
            .output();

        if let Ok(out) = prune_output {
            if out.status.success() {
                vm_println!("  Build cache: Reclaimed ~{} MB", cache_mb);
            }
        }
    }

    Ok(cache_mb)
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

/// Print cleanup summary
fn print_cleanup_summary(results: &CleanupResults, dry_run: bool) {
    let total =
        results.volumes + results.temp_containers + results.log_files + results.dangling_images;

    vm_println!("");

    if dry_run {
        if total == 0 && results.build_cache_mb == 0 {
            vm_println!("✓ Nothing to clean - system is already tidy!");
        } else {
            vm_println!("📋 Dry run complete: {} items would be removed", total);
            if results.build_cache_mb > 0 {
                vm_println!("   Plus ~{} MB of build cache", results.build_cache_mb);
            }
            vm_println!("\n💡 Run without --dry-run to actually clean");
        }
    } else if total == 0 && results.build_cache_mb == 0 {
        vm_success!("✓ Nothing to clean - system is already tidy!");
    } else {
        vm_success!("✓ Cleanup complete!");
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
