//! Client operations for Docker registry CLI commands

use crate::server::{check_registry_running, garbage_collect as gc_registry, get_registry_status};
use crate::types::RegistryConfig;
use anyhow::{anyhow, Result};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm};
use tracing::{debug, warn};

/// Show the status of the Docker registry
pub async fn status_registry() -> Result<()> {
    println!("🐳 Docker Registry Status");
    println!();

    let status = get_registry_status().await?;

    // Overall status
    if status.running {
        println!("  Status: {}", "Running".bright_green());
        if let Some(health_check) = status.last_health_check {
            println!(
                "  Last Health Check: {}",
                health_check.format("%Y-%m-%d %H:%M:%S UTC")
            );
        }
    } else {
        println!("  Status: {}", "Not running".bright_red());
    }

    println!();

    // Container status
    print_container_status("Proxy Container", &status.proxy_status);
    print_container_status("Backend Container", &status.backend_status);

    // Registry statistics
    if let Some(stats) = status.stats {
        println!("📊 Registry Statistics:");
        println!("  Repositories: {}", stats.repository_count);
        println!("  Images: {}", stats.image_count);
        println!("  Storage Used: {}", format_bytes(stats.storage_used_bytes));

        if let (Some(hits), Some(misses)) = (stats.cache_hits, stats.cache_misses) {
            let total = hits + misses;
            let hit_rate = if total > 0 {
                (hits as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            println!(
                "  Cache Hit Rate: {:.1}% ({}/{} pulls)",
                hit_rate, hits, total
            );
        }

        if let Some(last_gc) = stats.last_gc_time {
            println!("  Last GC: {}", last_gc.format("%Y-%m-%d %H:%M:%S UTC"));
        }
        println!();
    }

    // Service URLs
    if status.running {
        println!("🌐 Service URLs:");
        println!("  Registry: {}", "http://localhost:5000".bright_cyan());
        println!(
            "  Health Check: {}",
            "http://localhost:5000/health".bright_cyan()
        );
        println!();
    }

    // Usage instructions
    if status.running {
        println!("💡 Usage:");
        println!("  Configure VM: Add 'docker_registry: true' to vm.yaml");
        println!("  Test pull: docker pull ubuntu:20.04");
        println!("  Garbage collect: vm registry gc");
    } else {
        println!("💡 Start registry: vm registry start");
    }

    Ok(())
}

/// Run garbage collection on the registry
pub async fn garbage_collect(force: bool, dry_run: bool) -> Result<()> {
    // Check if registry is running
    if !check_registry_running(crate::DEFAULT_REGISTRY_PORT).await {
        return Err(anyhow!(
            "Docker registry is not running. Start it with: vm registry start"
        ));
    }

    if dry_run {
        println!("🔍 DRY RUN - Would perform garbage collection");
        println!("  Force delete untagged: {}", force);
        return Ok(());
    }

    // Confirm if not forced
    if !force {
        let confirm = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(
                "Run garbage collection? This will delete unused images and may take some time",
            )
            .default(false)
            .interact()?;

        if !confirm {
            println!("❌ Cancelled");
            return Ok(());
        }
    }

    println!("🗑️  Running garbage collection...");
    if force {
        println!("⚠️  Force mode: deleting untagged images");
    }

    let start_time = std::time::Instant::now();
    let result = gc_registry(force).await?;
    let duration = start_time.elapsed();

    println!();
    println!(
        "✅ Garbage collection completed in {:.1}s",
        duration.as_secs_f64()
    );
    println!();
    println!("📊 Results:");
    println!(
        "  Repositories processed: {}",
        result.repositories_processed
    );
    println!("  Images deleted: {}", result.images_deleted);
    println!("  Space freed: {}", format_bytes(result.bytes_freed));

    if !result.errors.is_empty() {
        println!();
        println!("⚠️  Errors encountered:");
        for error in &result.errors {
            println!("  • {}", error.bright_yellow());
        }
    }

    Ok(())
}

/// Show the current registry configuration
pub async fn show_config() -> Result<()> {
    println!("⚙️  Docker Registry Configuration");
    println!();

    let config = load_or_default_config().await?;

    println!("🌐 Network:");
    println!("  Registry Port: {}", config.registry_port);
    println!("  Backend Port: {}", config.backend_port);
    println!("  Host: {}", config.host);
    println!();

    println!("💾 Storage:");
    println!("  Data Directory: {}", config.data_dir);
    if let Some(max_size) = config.max_size_bytes {
        println!("  Max Size: {}", format_bytes(max_size));
    }
    println!();

    println!("🗑️  Garbage Collection:");
    println!("  Max Age: {} days", config.gc_policy.max_age_days);
    println!(
        "  LRU Cleanup: {}",
        if config.gc_policy.lru_cleanup {
            "Enabled"
        } else {
            "Disabled"
        }
    );
    println!(
        "  Auto GC: {}",
        if config.gc_policy.auto_gc {
            "Enabled"
        } else {
            "Disabled"
        }
    );
    if config.gc_policy.auto_gc {
        println!(
            "  GC Interval: {} hours",
            config.gc_policy.gc_interval_hours
        );
    }
    println!();

    println!(
        "🐛 Debug: {}",
        if config.debug { "Enabled" } else { "Disabled" }
    );

    Ok(())
}

/// List cached Docker images in the registry
pub async fn list_cached_images() -> Result<()> {
    // Check if registry is running
    if !check_registry_running(crate::DEFAULT_REGISTRY_PORT).await {
        println!("❌ Docker registry is not running");
        return Ok(());
    }

    println!("📦 Cached Docker Images");
    println!();

    // Try to query the registry API for image list
    // This is a simplified implementation - a full implementation would parse the registry API
    match query_registry_catalog().await {
        Ok(repositories) => {
            if repositories.is_empty() {
                println!("📭 No images cached yet");
                println!("\n💡 Pull an image to populate the cache: docker pull ubuntu:20.04");
            } else {
                println!("Found {} repositories:", repositories.len());
                for repo in repositories {
                    println!("  📦 {}", repo.bright_cyan());
                }
            }
        }
        Err(e) => {
            warn!("Failed to query registry catalog: {}", e);
            println!("⚠️  Unable to list cached images");
            println!("   Registry may be starting up or experiencing issues");
        }
    }

    Ok(())
}

/// Print container status information
fn print_container_status(name: &str, status: &crate::types::ContainerStatus) {
    println!("🐳 {}:", name);

    if status.exists {
        let status_color = if status.running {
            status.status.bright_green()
        } else {
            status.status.bright_red()
        };

        println!("  Status: {}", status_color);

        if let Some(id) = &status.container_id {
            println!("  Container ID: {}", id[..12].dimmed()); // Show short ID
        }

        if let Some(image) = &status.image {
            println!("  Image: {}", image.bright_blue());
        }

        if let Some(created) = &status.created_at {
            println!("  Created: {}", created.format("%Y-%m-%d %H:%M:%S UTC"));
        }
    } else {
        println!("  Status: {}", "Not found".bright_red());
    }

    println!();
}

/// Format bytes in human readable format
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Load configuration or return default
async fn load_or_default_config() -> Result<RegistryConfig> {
    // Try to load from config file, fallback to default
    // In a full implementation, this would check ~/.vm/registry/config.json
    Ok(RegistryConfig::default())
}

/// Query the registry catalog API
async fn query_registry_catalog() -> Result<Vec<String>> {
    let url = "http://localhost:5000/v2/_catalog";

    let response = reqwest::get(url)
        .await
        .map_err(|e| anyhow!("Failed to query registry catalog: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Registry catalog query failed: {}",
            response.status()
        ));
    }

    #[derive(serde::Deserialize)]
    struct CatalogResponse {
        repositories: Vec<String>,
    }

    let catalog: CatalogResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse catalog response: {}", e))?;

    Ok(catalog.repositories)
}

/// Auto-start registry if needed with user confirmation
pub async fn start_registry_if_needed() -> Result<()> {
    if check_registry_running(crate::DEFAULT_REGISTRY_PORT).await {
        debug!("Docker registry is already running");
        return Ok(());
    }

    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Docker registry is not running. Start it now?")
        .default(true)
        .interact()?;

    if confirm {
        println!("🚀 Starting Docker registry...");

        crate::server::start_registry().await?;

        // Verify it started
        if check_registry_running(crate::DEFAULT_REGISTRY_PORT).await {
            println!("✅ Docker registry started successfully");
        } else {
            return Err(anyhow!("Failed to start Docker registry"));
        }
    } else {
        return Err(anyhow!("Docker registry is required but not running"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }

    #[tokio::test]
    async fn test_load_default_config() {
        let config = load_or_default_config().await.unwrap();
        assert_eq!(config.registry_port, 5000);
        assert_eq!(config.backend_port, 5001);
        assert_eq!(config.host, "127.0.0.1");
    }

    #[tokio::test]
    async fn test_query_registry_catalog_error() {
        // Test with invalid URL - should return error
        let result = query_registry_catalog().await;
        // This will fail because the registry isn't running, which is expected
        assert!(result.is_err());
    }
}
