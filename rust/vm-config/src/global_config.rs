//! Global configuration for VM tool-wide settings
//!
//! This module defines the structure for the global ~/.vm/config.yaml file,
//! which contains settings that apply to all VMs on the system, such as
//! shared services (Docker registry, auth proxy, package registry) and
//! user-wide defaults.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Root structure for global VM tool configuration
///
/// This configuration is stored in ~/.vm/config.yaml and contains
/// settings that apply to all VMs on the system.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    /// Schema reference for IDE support
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Global services configuration
    #[serde(default, skip_serializing_if = "GlobalServices::is_default")]
    pub services: GlobalServices,

    /// Default values for VM configurations
    #[serde(default, skip_serializing_if = "GlobalDefaults::is_default")]
    pub defaults: GlobalDefaults,

    /// Global feature flags
    #[serde(default, skip_serializing_if = "GlobalFeatures::is_default")]
    pub features: GlobalFeatures,

    /// Git worktree settings
    #[serde(default, skip_serializing_if = "WorktreesGlobalSettings::is_default")]
    pub worktrees: WorktreesGlobalSettings,

    /// Backup settings
    #[serde(default, skip_serializing_if = "BackupSettings::is_default")]
    pub backups: BackupSettings,

    /// Extra configuration for extensions
    #[serde(flatten)]
    pub extra: IndexMap<String, serde_json::Value>,
}

/// Global backup settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSettings {
    /// Whether backups are enabled globally
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Directory to store backups
    #[serde(default = "default_backup_path")]
    pub path: String,

    /// Number of backups to keep per service
    #[serde(default = "default_keep_count")]
    pub keep_count: u32,

    /// Whether to only back up databases by default
    #[serde(default = "default_true")]
    pub databases_only: bool,
}

fn default_backup_path() -> String {
    "~/.vm/backups".to_string()
}

fn default_keep_count() -> u32 {
    5
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            path: default_backup_path(),
            keep_count: default_keep_count(),
            databases_only: true,
        }
    }
}

impl BackupSettings {
    /// Check if settings are at defaults
    pub fn is_default(&self) -> bool {
        self.enabled
            && self.path == default_backup_path()
            && self.keep_count == default_keep_count()
            && self.databases_only
    }
}

/// Global services that serve all VMs on the system
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalServices {
    /// Docker registry cache configuration
    #[serde(default, skip_serializing_if = "DockerRegistrySettings::is_default")]
    pub docker_registry: DockerRegistrySettings,

    /// Authentication proxy configuration
    #[serde(default, skip_serializing_if = "AuthProxySettings::is_default")]
    pub auth_proxy: AuthProxySettings,

    /// Package registry configuration
    #[serde(default, skip_serializing_if = "PackageRegistrySettings::is_default")]
    pub package_registry: PackageRegistrySettings,

    /// PostgreSQL service configuration
    #[serde(default, skip_serializing_if = "PostgresSettings::is_default")]
    pub postgresql: PostgresSettings,

    /// Redis service configuration
    #[serde(default, skip_serializing_if = "RedisSettings::is_default")]
    pub redis: RedisSettings,

    /// MongoDB service configuration
    #[serde(default, skip_serializing_if = "MongoDBSettings::is_default")]
    pub mongodb: MongoDBSettings,

    /// MySQL service configuration
    #[serde(default, skip_serializing_if = "MySqlSettings::is_default")]
    pub mysql: MySqlSettings,
}

impl GlobalServices {
    /// Check if all services are at default settings
    pub fn is_default(&self) -> bool {
        self.docker_registry.is_default()
            && self.auth_proxy.is_default()
            && self.package_registry.is_default()
            && self.postgresql.is_default()
            && self.redis.is_default()
            && self.mongodb.is_default()
            && self.mysql.is_default()
    }
}

/// PostgreSQL service settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresSettings {
    /// Whether the PostgreSQL service is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Port for the PostgreSQL service (default: 5432)
    #[serde(default = "default_postgres_port")]
    pub port: u16,

    /// Docker image version for PostgreSQL
    #[serde(default = "default_postgres_version")]
    pub version: String,

    /// Directory to store PostgreSQL data
    #[serde(default = "default_postgres_data_dir")]
    pub data_dir: String,

    /// Whether to automatically back up on destroy
    #[serde(default)]
    pub auto_backup: bool,

    /// Number of backups to keep (0 for infinite)
    #[serde(default = "default_backup_retention")]
    pub backup_retention: u32,
}

fn default_backup_retention() -> u32 {
    7
}

impl Default for PostgresSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_postgres_port(),
            version: default_postgres_version(),
            data_dir: default_postgres_data_dir(),
            auto_backup: false,
            backup_retention: default_backup_retention(),
        }
    }
}

impl PostgresSettings {
    /// Check if settings are at defaults
    pub fn is_default(&self) -> bool {
        !self.enabled && !self.auto_backup && self.backup_retention == default_backup_retention()
    }
}

/// Redis service settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisSettings {
    /// Whether the Redis service is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Port for the Redis service (default: 6379)
    #[serde(default = "default_redis_port")]
    pub port: u16,

    /// Docker image version for Redis
    #[serde(default = "default_redis_version")]
    pub version: String,

    /// Directory to store Redis data
    #[serde(default = "default_redis_data_dir")]
    pub data_dir: String,
}

impl Default for RedisSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_redis_port(),
            version: default_redis_version(),
            data_dir: default_redis_data_dir(),
        }
    }
}

impl RedisSettings {
    /// Check if settings are at defaults
    pub fn is_default(&self) -> bool {
        !self.enabled
    }
}

/// MongoDB service settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoDBSettings {
    /// Whether the MongoDB service is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Port for the MongoDB service (default: 27017)
    #[serde(default = "default_mongodb_port")]
    pub port: u16,

    /// Docker image version for MongoDB
    #[serde(default = "default_mongodb_version")]
    pub version: String,

    /// Directory to store MongoDB data
    #[serde(default = "default_mongodb_data_dir")]
    pub data_dir: String,
}

impl Default for MongoDBSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_mongodb_port(),
            version: default_mongodb_version(),
            data_dir: default_mongodb_data_dir(),
        }
    }
}

impl MongoDBSettings {
    /// Check if settings are at defaults
    pub fn is_default(&self) -> bool {
        !self.enabled
    }
}

/// MySQL service settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlSettings {
    /// Whether the MySQL service is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Port for the MySQL service (default: 3306)
    #[serde(default = "default_mysql_port")]
    pub port: u16,

    /// Docker image version for MySQL
    #[serde(default = "default_mysql_version")]
    pub version: String,

    /// Directory to store MySQL data
    #[serde(default = "default_mysql_data_dir")]
    pub data_dir: String,
}

impl Default for MySqlSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_mysql_port(),
            version: default_mysql_version(),
            data_dir: default_mysql_data_dir(),
        }
    }
}

impl MySqlSettings {
    /// Check if settings are at defaults
    pub fn is_default(&self) -> bool {
        !self.enabled
    }
}

/// Docker registry cache settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerRegistrySettings {
    /// Whether the registry is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Port for the registry (default: 5000)
    #[serde(default = "default_docker_registry_port")]
    pub port: u16,

    /// Maximum cache size in GB
    #[serde(default = "default_cache_size")]
    pub max_cache_size_gb: u64,

    /// Maximum age of cached images in days
    #[serde(default = "default_image_age")]
    pub max_image_age_days: u32,

    /// Cleanup interval in hours
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval_hours: u32,

    /// Enable LRU eviction when cache is full
    #[serde(default = "default_true")]
    pub enable_lru_eviction: bool,

    /// Auto-restart on failure
    #[serde(default = "default_true")]
    pub enable_auto_restart: bool,

    /// Health check interval in minutes
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_minutes: u32,
}

impl Default for DockerRegistrySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_docker_registry_port(),
            max_cache_size_gb: default_cache_size(),
            max_image_age_days: default_image_age(),
            cleanup_interval_hours: default_cleanup_interval(),
            enable_lru_eviction: true,
            enable_auto_restart: true,
            health_check_interval_minutes: default_health_check_interval(),
        }
    }
}

impl DockerRegistrySettings {
    /// Check if settings are at defaults
    pub fn is_default(&self) -> bool {
        !self.enabled
    }
}

/// Authentication proxy settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProxySettings {
    /// Whether the auth proxy is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Port for the auth proxy (default: 3090)
    #[serde(default = "default_auth_proxy_port")]
    pub port: u16,

    /// Token expiry in hours
    #[serde(default = "default_token_expiry")]
    pub token_expiry_hours: u32,
}

impl Default for AuthProxySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_auth_proxy_port(),
            token_expiry_hours: default_token_expiry(),
        }
    }
}

impl AuthProxySettings {
    /// Check if settings are at defaults
    pub fn is_default(&self) -> bool {
        !self.enabled
    }
}

/// Package registry settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRegistrySettings {
    /// Whether the package registry is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Port for the package registry (default: 3080)
    #[serde(default = "default_package_registry_port")]
    pub port: u16,

    /// Maximum storage size in GB
    #[serde(default = "default_package_storage")]
    pub max_storage_gb: u64,
}

impl Default for PackageRegistrySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_package_registry_port(),
            max_storage_gb: default_package_storage(),
        }
    }
}

impl PackageRegistrySettings {
    /// Check if settings are at defaults
    pub fn is_default(&self) -> bool {
        !self.enabled
    }
}

/// Global default values for VM configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalDefaults {
    /// Default provider when not specified
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Default terminal configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<crate::config::TerminalConfig>,

    /// Default memory allocation in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u32>,

    /// Default CPU count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<u32>,

    /// Default user in VMs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

impl GlobalDefaults {
    /// Check if all defaults are unset
    pub fn is_default(&self) -> bool {
        self.provider.is_none()
            && self.terminal.is_none()
            && self.memory.is_none()
            && self.cpus.is_none()
            && self.user.is_none()
    }
}

/// Global feature flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalFeatures {
    /// Enable automatic preset detection
    #[serde(default = "default_true")]
    pub auto_detect_presets: bool,

    /// Enable automatic port allocation
    #[serde(default = "default_true")]
    pub auto_port_allocation: bool,

    /// Enable telemetry (anonymous usage statistics)
    #[serde(default)]
    pub telemetry: bool,

    /// Enable update notifications
    #[serde(default = "default_true")]
    pub update_notifications: bool,
}

impl Default for GlobalFeatures {
    fn default() -> Self {
        Self {
            auto_detect_presets: true,
            auto_port_allocation: true,
            telemetry: false,
            update_notifications: true,
        }
    }
}

impl GlobalFeatures {
    /// Check if all features are at defaults
    pub fn is_default(&self) -> bool {
        self.auto_detect_presets
            && self.auto_port_allocation
            && !self.telemetry
            && self.update_notifications
    }
}

/// Global settings for git worktrees
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreesGlobalSettings {
    /// Enable worktrees for all projects by default
    #[serde(default = "default_worktrees_enabled")]
    pub enabled: bool,

    /// Default base path for worktree directories
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
}

impl Default for WorktreesGlobalSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            base_path: None,
        }
    }
}

impl WorktreesGlobalSettings {
    /// Check if settings are at defaults
    pub fn is_default(&self) -> bool {
        self.enabled && self.base_path.is_none()
    }
}

fn default_worktrees_enabled() -> bool {
    true
}

// Default value functions for serde
fn default_docker_registry_port() -> u16 {
    5000
}

fn default_auth_proxy_port() -> u16 {
    3090
}

fn default_package_registry_port() -> u16 {
    3080
}

fn default_postgres_port() -> u16 {
    5432
}

fn default_postgres_version() -> String {
    "16".to_string()
}

fn default_postgres_data_dir() -> String {
    "~/.vm/data/postgres".to_string()
}

fn default_redis_port() -> u16 {
    6379
}

fn default_redis_version() -> String {
    "7".to_string()
}

fn default_redis_data_dir() -> String {
    "~/.vm/data/redis".to_string()
}

fn default_mongodb_port() -> u16 {
    27017
}

fn default_mongodb_version() -> String {
    "7".to_string()
}

fn default_mongodb_data_dir() -> String {
    "~/.vm/data/mongodb".to_string()
}

fn default_mysql_port() -> u16 {
    3306
}

fn default_mysql_version() -> String {
    "8".to_string()
}

fn default_mysql_data_dir() -> String {
    "~/.vm/data/mysql".to_string()
}

fn default_cache_size() -> u64 {
    5
}

fn default_image_age() -> u32 {
    30
}

fn default_cleanup_interval() -> u32 {
    1
}

fn default_health_check_interval() -> u32 {
    15
}

fn default_token_expiry() -> u32 {
    24
}

fn default_package_storage() -> u64 {
    10
}

fn default_true() -> bool {
    true
}

impl GlobalConfig {
    /// Load global configuration from the standard location
    ///
    /// If the config file doesn't exist, creates it with default values automatically.
    pub fn load() -> vm_core::error::Result<Self> {
        let config_path = vm_core::user_paths::global_config_path()?;

        if !config_path.exists() {
            // Create default config file automatically
            let default_config = Self::default();
            default_config.save_to_path(&config_path)?;
            return Ok(default_config);
        }

        Self::load_from_path(&config_path)
    }

    /// Load global configuration from a specific path
    pub fn load_from_path(path: &std::path::Path) -> vm_core::error::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml_ng::from_str(&contents)?;
        Ok(config)
    }

    /// Save global configuration to the standard location
    pub fn save(&self) -> vm_core::error::Result<()> {
        let config_path = vm_core::user_paths::global_config_path()?;
        self.save_to_path(&config_path)
    }

    /// Save global configuration to a specific path
    pub fn save_to_path(&self, path: &std::path::Path) -> vm_core::error::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let yaml = serde_yaml_ng::to_string(self)?;
        std::fs::write(path, yaml)?;

        Ok(())
    }
}
