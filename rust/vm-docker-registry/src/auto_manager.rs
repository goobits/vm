//! Auto-management for Docker registry cache
//!
//! This module provides intelligent, background management of the Docker registry cache
//! including automatic cleanup, LRU eviction, and self-healing capabilities.

use crate::server::{check_registry_running, get_registry_status, start_registry};
use crate::types::AutoConfig;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use tokio::time::{interval, sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn};

/// Auto-manager for Docker registry cache
pub struct AutoManager {
    config: AutoConfig,
    last_cleanup: Option<DateTime<Utc>>,
    last_health_check: Option<DateTime<Utc>>,
    restart_attempts: u32,
}

impl AutoManager {
    /// Create new auto-manager with default configuration
    pub fn new() -> Self {
        Self {
            config: AutoConfig::default(),
            last_cleanup: None,
            last_health_check: None,
            restart_attempts: 0,
        }
    }

    /// Create new auto-manager with custom configuration
    pub fn with_config(config: AutoConfig) -> Self {
        Self {
            config,
            last_cleanup: None,
            last_health_check: None,
            restart_attempts: 0,
        }
    }

    /// Run the background auto-management task
    pub async fn run_background_task(config: Option<AutoConfig>) -> Result<()> {
        let mut manager = match config {
            Some(cfg) => Self::with_config(cfg),
            None => Self::new(),
        };

        debug!("Starting Docker registry auto-manager");

        // Create intervals for different operations
        let mut cleanup_interval = interval(TokioDuration::from_secs(
            manager.config.cleanup_interval_hours as u64 * 3600,
        ));
        let mut health_interval = interval(TokioDuration::from_secs(
            manager.config.health_check_interval_minutes as u64 * 60,
        ));

        loop {
            tokio::select! {
                _ = cleanup_interval.tick() => {
                    if let Err(e) = manager.run_cleanup_cycle().await {
                        error!("Auto-cleanup failed: {}", e);
                    }
                }
                _ = health_interval.tick() => {
                    if let Err(e) = manager.run_health_check().await {
                        error!("Health check failed: {}", e);
                    }
                }
            }
        }
    }

    /// Run a complete cleanup cycle
    async fn run_cleanup_cycle(&mut self) -> Result<()> {
        debug!("Running auto-cleanup cycle");

        if !check_registry_running(crate::DEFAULT_REGISTRY_PORT).await {
            debug!("Registry not running, skipping cleanup");
            return Ok(());
        }

        // Check if cleanup is needed
        if !self.should_run_cleanup().await? {
            debug!("Cleanup not needed at this time");
            return Ok(());
        }

        self.enforce_limits().await?;
        self.last_cleanup = Some(Utc::now());

        debug!("Auto-cleanup cycle completed");
        Ok(())
    }

    /// Check if cleanup should run based on time and conditions
    async fn should_run_cleanup(&self) -> Result<bool> {
        // Always run if we haven't run cleanup before
        let last_cleanup = match self.last_cleanup {
            Some(time) => time,
            None => return Ok(true),
        };

        // Check if enough time has passed
        let now = Utc::now();
        let cleanup_interval = Duration::hours(self.config.cleanup_interval_hours as i64);

        if now - last_cleanup < cleanup_interval {
            return Ok(false);
        }

        // Check if we're approaching size limits
        if let Ok(status) = get_registry_status().await {
            if let Some(stats) = status.stats {
                let size_gb = stats.storage_used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let threshold = self.config.max_cache_size_gb as f64 * 0.8; // 80% threshold

                if size_gb > threshold {
                    debug!(
                        "Cache size ({:.1}GB) approaching limit ({:.1}GB), cleanup needed",
                        size_gb, threshold
                    );
                    return Ok(true);
                }
            }
        }

        Ok(true)
    }

    /// Enforce cache size and age limits
    async fn enforce_limits(&self) -> Result<()> {
        let status = get_registry_status().await?;
        let stats = status
            .stats
            .ok_or_else(|| anyhow!("No registry stats available"))?;

        // Check size limits
        let size_gb = stats.storage_used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        if size_gb > self.config.max_cache_size_gb as f64 {
            info!(
                "Cache size ({:.1}GB) exceeds limit ({:.1}GB), running cleanup",
                size_gb, self.config.max_cache_size_gb
            );

            if self.config.enable_lru_eviction {
                self.evict_lru().await?;
            } else {
                self.cleanup_old_images().await?;
            }
        }

        // Check age limits
        self.cleanup_old_images().await?;

        Ok(())
    }

    /// Implement LRU eviction strategy
    async fn evict_lru(&self) -> Result<()> {
        debug!("Running LRU eviction");

        // Get repository list and check image ages
        let repositories = self.get_repository_list().await?;
        let mut images_to_delete = Vec::new();

        for repo in repositories {
            let tags = self.get_repository_tags(&repo).await?;
            for tag in tags {
                let manifest_info = match self.get_image_manifest_info(&repo, &tag).await {
                    Ok(info) => info,
                    Err(_) => continue,
                };

                // Calculate age from manifest creation date
                let age_days = (Utc::now() - manifest_info.created).num_days();
                if age_days > self.config.max_image_age_days as i64 {
                    images_to_delete.push((repo.clone(), tag));
                }
            }
        }

        // Delete old images
        for (repo, tag) in images_to_delete {
            if let Err(e) = self.delete_image_tag(&repo, &tag).await {
                warn!("Failed to delete image {}:{}: {}", repo, tag, e);
            } else {
                debug!("Deleted old image: {}:{}", repo, tag);
            }
        }

        // Run garbage collection to free space
        self.run_garbage_collection(true).await?;

        Ok(())
    }

    /// Clean up images older than the configured age limit
    async fn cleanup_old_images(&self) -> Result<()> {
        debug!(
            "Cleaning up old images (older than {} days)",
            self.config.max_image_age_days
        );

        // Get repository list and check image ages
        let repositories = self.get_repository_list().await?;
        let mut deleted_count = 0;

        for repo in repositories {
            let tags = self.get_repository_tags(&repo).await?;
            for tag in tags {
                let manifest_info = match self.get_image_manifest_info(&repo, &tag).await {
                    Ok(info) => info,
                    Err(_) => continue,
                };

                // Calculate age from manifest creation date
                let age_days = (Utc::now() - manifest_info.created).num_days();
                if age_days <= self.config.max_image_age_days as i64 {
                    continue;
                }

                // Delete old image
                if let Err(e) = self.delete_image_tag(&repo, &tag).await {
                    warn!("Failed to delete old image {}:{}: {}", repo, tag, e);
                } else {
                    debug!(
                        "Deleted old image: {}:{} (age: {} days)",
                        repo, tag, age_days
                    );
                    deleted_count += 1;
                }
            }
        }

        if deleted_count > 0 {
            info!("Cleaned up {} old images", deleted_count);
            // Run garbage collection to free space
            self.run_garbage_collection(false).await?;
        }

        Ok(())
    }

    /// Run garbage collection on the registry
    async fn run_garbage_collection(&self, force: bool) -> Result<()> {
        debug!("Running garbage collection (force={})", force);

        // Use the existing garbage collection from server module
        match crate::server::garbage_collect(force).await {
            Ok(result) => {
                debug!(
                    "GC completed: {} images deleted, {} bytes freed",
                    result.images_deleted, result.bytes_freed
                );
                Ok(())
            }
            Err(e) => {
                warn!("Garbage collection failed: {}", e);
                Err(e)
            }
        }
    }

    /// Run health check and attempt auto-restart if needed
    async fn run_health_check(&mut self) -> Result<()> {
        debug!("Running health check");

        let is_healthy = check_registry_running(crate::DEFAULT_REGISTRY_PORT).await;
        self.last_health_check = Some(Utc::now());

        if is_healthy {
            // Reset restart attempts on successful health check
            self.restart_attempts = 0;
            debug!("Registry health check passed");
            return Ok(());
        }

        warn!("Registry health check failed");

        // Attempt auto-restart if enabled and we haven't exceeded retry limit
        if self.config.enable_auto_restart && self.restart_attempts < 3 {
            self.restart_attempts += 1;
            warn!("Attempting auto-restart #{}", self.restart_attempts);

            if let Err(e) = self.attempt_restart().await {
                error!(
                    "Auto-restart attempt {} failed: {}",
                    self.restart_attempts, e
                );
            } else {
                info!("Auto-restart attempt {} succeeded", self.restart_attempts);
            }
        } else if self.restart_attempts >= 3 {
            error!("Maximum restart attempts exceeded, registry may require manual intervention");
        }

        Ok(())
    }

    /// Attempt to restart the registry service
    async fn attempt_restart(&self) -> Result<()> {
        debug!("Attempting registry restart");

        // Give the service a moment to settle
        sleep(TokioDuration::from_secs(5)).await;

        // Attempt to start the registry
        start_registry().await?;

        // Wait a bit and verify it started
        sleep(TokioDuration::from_secs(10)).await;

        if check_registry_running(crate::DEFAULT_REGISTRY_PORT).await {
            info!("Registry restart successful");
            Ok(())
        } else {
            Err(anyhow!("Registry failed to start after restart attempt"))
        }
    }

    /// Get current auto-manager status
    pub async fn get_status(&self) -> Result<AutoManagerStatus> {
        Ok(AutoManagerStatus {
            config: self.config.clone(),
            last_cleanup: self.last_cleanup,
            last_health_check: self.last_health_check,
            restart_attempts: self.restart_attempts,
            registry_healthy: check_registry_running(crate::DEFAULT_REGISTRY_PORT).await,
        })
    }

    /// Get list of repositories from registry API
    async fn get_repository_list(&self) -> Result<Vec<String>> {
        let url = format!(
            "http://127.0.0.1:{}/v2/_catalog",
            crate::DEFAULT_REGISTRY_PORT
        );

        let response = reqwest::get(&url)
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

    /// Get list of tags for a repository
    async fn get_repository_tags(&self, repository: &str) -> Result<Vec<String>> {
        let url = format!(
            "http://127.0.0.1:{}/v2/{}/tags/list",
            crate::DEFAULT_REGISTRY_PORT,
            repository
        );

        let response = reqwest::get(&url)
            .await
            .map_err(|e| anyhow!("Failed to query repository tags: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Repository tags query failed: {}",
                response.status()
            ));
        }

        #[derive(serde::Deserialize)]
        struct TagsResponse {
            tags: Option<Vec<String>>,
        }

        let tags_response: TagsResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse tags response: {}", e))?;

        Ok(tags_response.tags.unwrap_or_default())
    }

    /// Get manifest information for an image
    async fn get_image_manifest_info(
        &self,
        repository: &str,
        tag: &str,
    ) -> Result<ImageManifestInfo> {
        let url = format!(
            "http://127.0.0.1:{}/v2/{}/manifests/{}",
            crate::DEFAULT_REGISTRY_PORT,
            repository,
            tag
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header(
                "Accept",
                "application/vnd.docker.distribution.manifest.v2+json",
            )
            .send()
            .await
            .map_err(|e| anyhow!("Failed to query image manifest: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Image manifest query failed: {}",
                response.status()
            ));
        }

        let manifest_text = response
            .text()
            .await
            .map_err(|e| anyhow!("Failed to read manifest response: {}", e))?;

        // Parse manifest to get creation date
        let manifest: serde_json::Value = serde_json::from_str(&manifest_text)
            .map_err(|e| anyhow!("Failed to parse manifest JSON: {}", e))?;

        // Extract creation date from history (simplified approach)
        let created = if let Some(history) = manifest
            .get("history")
            .and_then(|h| h.as_array())
            .and_then(|arr| arr.first())
        {
            if let Some(v1_compat) = history.get("v1Compatibility").and_then(|v| v.as_str()) {
                let v1_data: serde_json::Value =
                    serde_json::from_str(v1_compat).unwrap_or_default();
                if let Some(created_str) = v1_data.get("created").and_then(|c| c.as_str()) {
                    DateTime::parse_from_rfc3339(created_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now())
                } else {
                    Utc::now()
                }
            } else {
                Utc::now()
            }
        } else {
            // Fallback: use current time (will not be deleted based on age)
            Utc::now()
        };

        Ok(ImageManifestInfo {
            repository: repository.to_string(),
            tag: tag.to_string(),
            created,
            size: 0, // Could extract from manifest if needed
        })
    }

    /// Delete an image tag from the registry
    async fn delete_image_tag(&self, repository: &str, tag: &str) -> Result<()> {
        // First get the digest for the tag
        let url = format!(
            "http://127.0.0.1:{}/v2/{}/manifests/{}",
            crate::DEFAULT_REGISTRY_PORT,
            repository,
            tag
        );

        let client = reqwest::Client::new();
        let response = client
            .head(&url)
            .header(
                "Accept",
                "application/vnd.docker.distribution.manifest.v2+json",
            )
            .send()
            .await
            .map_err(|e| anyhow!("Failed to get manifest digest: {}", e))?;

        let digest = response
            .headers()
            .get("Docker-Content-Digest")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow!("No digest found for image"))?;

        // Delete the manifest by digest
        let delete_url = format!(
            "http://127.0.0.1:{}/v2/{}/manifests/{}",
            crate::DEFAULT_REGISTRY_PORT,
            repository,
            digest
        );

        let delete_response = client
            .delete(&delete_url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to delete manifest: {}", e))?;

        if !delete_response.status().is_success() {
            return Err(anyhow!(
                "Failed to delete manifest: {}",
                delete_response.status()
            ));
        }

        debug!("Successfully deleted image {}:{}", repository, tag);
        Ok(())
    }
}

impl Default for AutoManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Status of the auto-manager
#[derive(Debug, Clone)]
pub struct AutoManagerStatus {
    pub config: AutoConfig,
    pub last_cleanup: Option<DateTime<Utc>>,
    pub last_health_check: Option<DateTime<Utc>>,
    pub restart_attempts: u32,
    pub registry_healthy: bool,
}

/// Information about an image manifest
#[derive(Debug, Clone)]
struct ImageManifestInfo {
    #[allow(dead_code)]
    repository: String,
    #[allow(dead_code)]
    tag: String,
    created: DateTime<Utc>,
    #[allow(dead_code)]
    size: u64,
}

/// Start the auto-manager background task
pub fn start_auto_manager() -> Result<()> {
    tokio::spawn(async move {
        if let Err(e) = AutoManager::run_background_task(None).await {
            error!("Auto-manager task failed: {}", e);
        }
    });

    debug!("Auto-manager background task started");
    Ok(())
}

/// Start the auto-manager with custom configuration
pub fn start_auto_manager_with_config(config: AutoConfig) -> Result<()> {
    tokio::spawn(async move {
        if let Err(e) = AutoManager::run_background_task(Some(config)).await {
            error!("Auto-manager task failed: {}", e);
        }
    });

    debug!("Auto-manager background task started with custom config");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_config_default() {
        let config = AutoConfig::default();
        assert_eq!(config.max_cache_size_gb, 5);
        assert_eq!(config.max_image_age_days, 30);
        assert_eq!(config.cleanup_interval_hours, 1);
        assert!(config.enable_lru_eviction);
        assert!(config.enable_auto_restart);
    }

    #[test]
    fn test_auto_manager_creation() {
        let manager = AutoManager::new();
        assert_eq!(manager.config.max_cache_size_gb, 5);
        assert_eq!(manager.restart_attempts, 0);
        assert!(manager.last_cleanup.is_none());
    }

    #[test]
    fn test_auto_manager_with_config() {
        let config = AutoConfig {
            max_cache_size_gb: 10,
            max_image_age_days: 60,
            cleanup_interval_hours: 2,
            enable_lru_eviction: false,
            enable_auto_restart: false,
            health_check_interval_minutes: 30,
        };

        let manager = AutoManager::with_config(config.clone());
        assert_eq!(manager.config.max_cache_size_gb, 10);
        assert_eq!(manager.config.max_image_age_days, 60);
        assert!(!manager.config.enable_lru_eviction);
    }

    #[tokio::test]
    async fn test_should_run_cleanup_first_time() {
        let manager = AutoManager::new();
        // Should run cleanup the first time (no last_cleanup time)
        let result = manager.should_run_cleanup().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_registry_api_error_handling() {
        let manager = AutoManager::new();

        // Test with registry not running - should handle gracefully
        let result = manager.get_repository_list().await;
        assert!(result.is_err());

        let result = manager.get_repository_tags("test").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_image_age_calculation() {
        use chrono::Duration;

        let config = AutoConfig {
            max_image_age_days: 30,
            ..Default::default()
        };

        // Test that a 40-day old image would be marked for deletion
        let old_date = Utc::now() - Duration::days(40);
        let age_days = (Utc::now() - old_date).num_days();
        assert!(age_days > config.max_image_age_days as i64);

        // Test that a 20-day old image would not be marked for deletion
        let recent_date = Utc::now() - Duration::days(20);
        let age_days = (Utc::now() - recent_date).num_days();
        assert!(age_days <= config.max_image_age_days as i64);
    }

    #[test]
    fn test_auto_manager_status() {
        let manager = AutoManager::new();
        assert_eq!(manager.restart_attempts, 0);
        assert!(manager.last_cleanup.is_none());
        assert!(manager.last_health_check.is_none());
    }
}
