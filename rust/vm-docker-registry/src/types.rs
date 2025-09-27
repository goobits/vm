//! Type definitions for Docker registry service

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Configuration for the Docker registry service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Registry proxy port (default: 5000)
    pub registry_port: u16,
    /// Backend registry port (default: 5001)
    pub backend_port: u16,
    /// Host to bind to (default: 127.0.0.1)
    pub host: String,
    /// Data directory for registry storage
    pub data_dir: String,
    /// Garbage collection policy
    pub gc_policy: GcPolicy,
    /// Maximum registry size in bytes
    pub max_size_bytes: Option<u64>,
    /// Whether to enable debug logging
    pub debug: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            registry_port: 5000,
            backend_port: 5001,
            host: "127.0.0.1".to_string(),
            data_dir: "~/.vm/registry".to_string(),
            gc_policy: GcPolicy::default(),
            max_size_bytes: Some(50 * 1024 * 1024 * 1024), // 50GB
            debug: false,
        }
    }
}

/// Garbage collection policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcPolicy {
    /// Delete unused images after this many days
    pub max_age_days: u32,
    /// Enable least-recently-used cleanup
    pub lru_cleanup: bool,
    /// Run GC automatically
    pub auto_gc: bool,
    /// GC interval in hours
    pub gc_interval_hours: u32,
}

impl Default for GcPolicy {
    fn default() -> Self {
        Self {
            max_age_days: 30,
            lru_cleanup: true,
            auto_gc: true,
            gc_interval_hours: 24,
        }
    }
}

/// Status of the Docker registry service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStatus {
    /// Whether the registry is running
    pub running: bool,
    /// Proxy container status
    pub proxy_status: ContainerStatus,
    /// Backend container status
    pub backend_status: ContainerStatus,
    /// Registry statistics
    pub stats: Option<RegistryStats>,
    /// Last health check
    pub last_health_check: Option<DateTime<Utc>>,
}

/// Status of a Docker container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStatus {
    /// Container name
    pub name: String,
    /// Whether container exists
    pub exists: bool,
    /// Whether container is running
    pub running: bool,
    /// Container ID if running
    pub container_id: Option<String>,
    /// Container image
    pub image: Option<String>,
    /// Creation time
    pub created_at: Option<DateTime<Utc>>,
    /// Status message
    pub status: String,
}

/// Registry usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    /// Total number of repositories
    pub repository_count: u32,
    /// Total number of images/tags
    pub image_count: u32,
    /// Total storage used in bytes
    pub storage_used_bytes: u64,
    /// Number of pulls served from cache
    pub cache_hits: Option<u64>,
    /// Number of pulls that went to upstream
    pub cache_misses: Option<u64>,
    /// Last garbage collection time
    pub last_gc_time: Option<DateTime<Utc>>,
}

/// Information about a Docker container
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    /// Container ID
    pub id: String,
    /// Container name
    pub name: String,
    /// Container image
    pub image: String,
    /// Container status
    pub status: String,
    /// Created timestamp
    pub created: String,
    /// Ports mapping
    pub ports: String,
}

/// Docker registry health check response
#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
}

/// Registry garbage collection result
#[derive(Debug, Clone, Serialize)]
pub struct GcResult {
    /// Number of repositories processed
    pub repositories_processed: u32,
    /// Number of images deleted
    pub images_deleted: u32,
    /// Bytes freed
    pub bytes_freed: u64,
    /// Duration of GC operation in seconds
    pub duration_seconds: u64,
    /// Any errors encountered
    pub errors: Vec<String>,
}

/// Docker daemon configuration for VMs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Registry mirrors
    #[serde(rename = "registry-mirrors")]
    pub registry_mirrors: Vec<String>,
    /// Insecure registries
    #[serde(rename = "insecure-registries")]
    pub insecure_registries: Vec<String>,
}

impl DaemonConfig {
    /// Create daemon config for VM with registry mirror
    pub fn for_registry(registry_url: &str) -> Self {
        Self {
            registry_mirrors: vec![registry_url.to_string()],
            insecure_registries: vec![registry_url.to_string()],
        }
    }
}
