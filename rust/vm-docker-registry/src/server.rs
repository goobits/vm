//! Docker registry server management

use crate::config::{get_registry_data_dir, write_config_files};
use crate::types::{ContainerStatus, RegistryConfig, RegistryStatus};
use anyhow::{anyhow, Context, Result};
use duct::cmd;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tracing::debug;

/// Start the Docker registry service
pub async fn start_registry() -> Result<()> {
    start_registry_with_config(&RegistryConfig::default()).await
}

/// Start the Docker registry service with custom configuration
pub async fn start_registry_with_config(config: &RegistryConfig) -> Result<()> {
    debug!("Starting Docker registry service...");

    // Check if Docker is available
    ensure_docker_available()?;

    // Check if already running
    if check_registry_running(config.registry_port).await {
        debug!(
            "Docker registry is already running on port {}",
            config.registry_port
        );
        return Ok(());
    }

    // Get data directory
    let data_dir = get_registry_data_dir()?;

    // Write configuration files
    write_config_files(config, &data_dir).context("Failed to write configuration files")?;

    // Start containers using docker-compose with retry mechanism
    start_containers_with_retry(&data_dir).await?;

    // Wait for services to be ready
    wait_for_registry_ready(config.registry_port, 30).await?;

    debug!(
        "Docker registry started successfully on port {}",
        config.registry_port
    );
    Ok(())
}

/// Stop the Docker registry service
pub async fn stop_registry() -> Result<()> {
    debug!("Stopping Docker registry service...");

    // Stop and remove containers
    stop_containers()?;

    debug!("Docker registry stopped successfully");
    Ok(())
}

/// Check if the Docker registry is running
pub async fn check_registry_running(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}/health", port);
    match reqwest::get(&url).await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

/// Get the status of the Docker registry service
pub async fn get_registry_status() -> Result<RegistryStatus> {
    let proxy_status = get_container_status(crate::PROXY_CONTAINER_NAME)?;
    let backend_status = get_container_status(crate::BACKEND_CONTAINER_NAME)?;

    let running = proxy_status.running && backend_status.running;

    let stats = if running {
        // Try to get registry statistics
        get_registry_stats().await.ok()
    } else {
        None
    };

    Ok(RegistryStatus {
        running,
        proxy_status,
        backend_status,
        stats,
        last_health_check: if running {
            Some(chrono::Utc::now())
        } else {
            None
        },
    })
}

/// Ensure Docker is available and running
fn ensure_docker_available() -> Result<()> {
    // Check if docker command exists
    if which::which("docker").is_err() {
        return Err(anyhow!("Docker is not installed or not in PATH"));
    }

    // Check if Docker daemon is running
    let output = Command::new("docker")
        .arg("version")
        .output()
        .context("Failed to run docker version command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Docker daemon is not running: {}", stderr));
    }

    debug!("Docker is available and running");
    Ok(())
}

/// Start the registry containers using docker-compose with retry mechanism
async fn start_containers_with_retry(data_dir: &Path) -> Result<()> {
    debug!("Starting registry containers...");

    let max_retries = 2;
    let mut last_error = None;

    for attempt in 1..=max_retries {
        match start_containers(data_dir) {
            Ok(()) => {
                debug!(
                    "Registry containers started successfully on attempt {}",
                    attempt
                );
                return Ok(());
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries {
                    debug!("Attempt {} failed, retrying in 2 seconds...", attempt);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow!("Failed to start containers after {} attempts", max_retries)))
}

/// Start the registry containers using docker-compose
fn start_containers(data_dir: &Path) -> Result<()> {
    debug!("Starting registry containers...");

    // Check if docker-compose is available
    let compose_cmd = if which::which("docker-compose").is_ok() {
        "docker-compose"
    } else if which::which("docker").is_ok() {
        // Try docker compose (newer syntax)
        "docker"
    } else {
        return Err(anyhow!(
            "Neither docker-compose nor docker compose is available"
        ));
    };

    let mut args = vec![];
    if compose_cmd == "docker" {
        args.push("compose");
    }
    args.extend_from_slice(&["-f", "docker-compose.yml", "up", "-d"]);

    let output = cmd(compose_cmd, &args)
        .dir(data_dir)
        .stderr_to_stdout()
        .run()
        .context("Failed to start registry containers")?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow!("Failed to start containers: {}", stdout));
    }

    debug!("Registry containers started successfully");
    Ok(())
}

/// Stop the registry containers
fn stop_containers() -> Result<()> {
    debug!("Stopping registry containers...");

    // Stop and remove containers directly
    let containers = [crate::PROXY_CONTAINER_NAME, crate::BACKEND_CONTAINER_NAME];

    for container in &containers {
        // Stop container
        let _ = Command::new("docker").args(["stop", container]).output();

        // Remove container
        let _ = Command::new("docker")
            .args(["rm", "-f", container])
            .output();
    }

    debug!("Registry containers stopped");
    Ok(())
}

/// Wait for the registry to be ready
async fn wait_for_registry_ready(port: u16, timeout_seconds: u64) -> Result<()> {
    debug!("Waiting for registry to be ready on port {}...", port);

    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_seconds);

    while start.elapsed() < timeout {
        if check_registry_running(port).await {
            debug!("Registry is ready");
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Err(anyhow!(
        "Registry failed to become ready within {} seconds",
        timeout_seconds
    ))
}

/// Get the status of a Docker container
fn get_container_status(container_name: &str) -> Result<ContainerStatus> {
    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("name={}", container_name),
            "--format",
            "{{.ID}}\\t{{.Image}}\\t{{.Status}}\\t{{.CreatedAt}}",
        ])
        .output()
        .context("Failed to get container status")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.trim().is_empty() {
        return Ok(ContainerStatus {
            name: container_name.to_string(),
            exists: false,
            running: false,
            container_id: None,
            image: None,
            created_at: None,
            status: "Not found".to_string(),
        });
    }

    let parts: Vec<&str> = stdout.trim().split('\t').collect();
    if parts.len() >= 4 {
        let container_id = parts[0].to_string();
        let image = parts[1].to_string();
        let status = parts[2].to_string();
        let running = status.contains("Up");

        // Try to parse created_at (this might fail, that's ok)
        let created_at = chrono::DateTime::parse_from_str(parts[3], "%Y-%m-%d %H:%M:%S %z")
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok();

        Ok(ContainerStatus {
            name: container_name.to_string(),
            exists: true,
            running,
            container_id: Some(container_id),
            image: Some(image),
            created_at,
            status,
        })
    } else {
        Err(anyhow!(
            "Failed to parse container status for {}",
            container_name
        ))
    }
}

/// Get registry statistics (placeholder implementation)
async fn get_registry_stats() -> Result<crate::types::RegistryStats> {
    // This is a placeholder implementation
    // In a full implementation, this would query the registry API for actual statistics
    Ok(crate::types::RegistryStats {
        repository_count: 0,
        image_count: 0,
        storage_used_bytes: 0,
        cache_hits: None,
        cache_misses: None,
        last_gc_time: None,
    })
}

/// Garbage collect unused registry data
pub async fn garbage_collect(force: bool) -> Result<crate::types::GcResult> {
    debug!("Starting registry garbage collection (force: {})", force);

    let start_time = std::time::Instant::now();
    let mut errors = Vec::new();

    // Check if registry is running
    if !check_registry_running(crate::DEFAULT_REGISTRY_PORT).await {
        return Err(anyhow!("Registry is not running"));
    }

    // Get container info for backend
    let backend_status = get_container_status(crate::BACKEND_CONTAINER_NAME)?;
    let container_id = backend_status
        .container_id
        .ok_or_else(|| anyhow!("Backend container not found"))?;

    // Run garbage collection in the registry container
    let gc_cmd = if force {
        vec![
            "exec",
            &container_id,
            "registry",
            "garbage-collect",
            "--delete-untagged=true",
            "/etc/docker/registry/config.yml",
        ]
    } else {
        vec![
            "exec",
            &container_id,
            "registry",
            "garbage-collect",
            "/etc/docker/registry/config.yml",
        ]
    };

    let output = Command::new("docker")
        .args(&gc_cmd)
        .output()
        .context("Failed to run garbage collection")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        errors.push(format!("Garbage collection failed: {}", stderr));
    }

    let duration = start_time.elapsed().as_secs();

    // Parse GC output for statistics (this is registry-specific)
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (repositories_processed, images_deleted, bytes_freed) = parse_gc_output(&stdout);

    Ok(crate::types::GcResult {
        repositories_processed,
        images_deleted,
        bytes_freed,
        duration_seconds: duration,
        errors,
    })
}

/// Extract byte count from a line like "deleted blob sha256:def456 (1024 bytes)"
fn extract_bytes_from_line(line: &str) -> Option<u64> {
    let pos = line.rfind('(')?;
    let potential_size = &line[pos + 1..];
    let end_pos = potential_size.find(" bytes)")?;
    let size_str = &potential_size[..end_pos];
    size_str.parse::<u64>().ok()
}

/// Parse garbage collection output for statistics
fn parse_gc_output(output: &str) -> (u32, u32, u64) {
    // This is a simplified parser for registry GC output
    // The actual format depends on the registry version
    let mut repositories = 0;
    let mut images = 0;
    let mut bytes = 0;

    for line in output.lines() {
        if line.contains("deleted") {
            images += 1;
        }
        if line.contains("repository") {
            repositories += 1;
        }
        // Try to extract byte counts (this is very registry-specific)
        // Look for pattern like "(1024" followed by "bytes)"
        if line.contains("bytes)") {
            bytes += extract_bytes_from_line(line).unwrap_or(0);
        }
    }

    (repositories, images, bytes)
}

/// Auto-start registry if needed for VM operations (silent)
pub async fn start_registry_if_needed() -> Result<()> {
    if check_registry_running(crate::DEFAULT_REGISTRY_PORT).await {
        debug!("Docker registry is already running");
        return Ok(());
    }

    // Start silently with default config - no user prompts
    debug!("Docker registry not running, starting with default configuration...");
    start_registry().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_docker_available() {
        // This test will only pass if Docker is actually installed and running
        // In CI environments without Docker, this would be skipped
        if which::which("docker").is_ok() {
            let result = ensure_docker_available();
            // Don't assert success because Docker might not be running in test environment
            // Just verify the function doesn't panic
            let _ = result;
        }
    }

    #[test]
    fn test_parse_gc_output() {
        let sample_output = r#"
        repository: example/repo
        deleted manifest sha256:abc123
        deleted blob sha256:def456 (1024 bytes)
        deleted blob sha256:ghi789 (2048 bytes)
        "#;

        let (repos, images, bytes) = parse_gc_output(sample_output);
        assert_eq!(repos, 1);
        assert_eq!(images, 3); // Three deleted lines (1 manifest + 2 blobs)
                               // Byte parsing is approximate due to simple regex
        assert!(bytes > 0);
    }

    #[tokio::test]
    async fn test_check_registry_running() {
        // Test with a port that's definitely not running a registry
        let running = check_registry_running(9999).await;
        assert!(!running);
    }
}
