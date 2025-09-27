//! # Configuration Management
//!
//! This module provides comprehensive configuration management for the package registry server.
//! It defines the configuration structure for all aspects of the server including:
//!
//! - Server settings (host, port, scheme)
//! - Storage configuration for different package types
//! - Route definitions for API endpoints
//! - Client configuration for package managers
//! - Security and limits configuration
//!
//! ## Configuration Structure
//!
//! The configuration is hierarchical and supports JSON serialization/deserialization.
//! The main [`Config`] struct contains all configuration sections:
//!
//! - [`ServerConfig`]: Basic server settings
//! - [`StorageConfig`]: File storage locations and directory structure
//! - [`RouteConfig`]: URL route patterns for different package types
//! - [`ClientConfig`]: Configuration file templates for package managers
//! - [`LimitsConfig`]: Upload limits and rate limiting
//! - [`SecurityConfig`]: Authentication and authorization settings
//!
//! ## Loading Configuration
//!
//! Configuration can be loaded from a JSON file or use sensible defaults:
//!
//! ```rust,no_run
//! # use crate::config::Config;
//! // Load from file with fallback to defaults
//! let config = Config::load_or_default("config.json")?;
//!
//! // Load from file (fails if file doesn't exist)
//! let config = Config::load("config.json")?;
//!
//! // Use built-in defaults
//! let config = Config::default();
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Main configuration structure for the package registry server.
///
/// This struct contains all configuration sections needed to run the server,
/// including server settings, storage locations, route patterns, client configurations,
/// and security settings. All fields support JSON serialization/deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration (host, port, scheme)
    pub server: ServerConfig,
    /// Storage paths and directory structure
    pub storage: StorageConfig,
    /// URL route patterns for different package types
    #[serde(default = "default_routes")]
    pub routes: RouteConfig,
    /// Client configuration templates for package managers
    #[serde(default = "default_client_config")]
    pub client_config: ClientConfig,
    /// External service URLs (upstream registries)
    pub external_urls: ExternalUrls,
    /// Upload and request limits (defaults applied if not specified)
    #[serde(default)]
    pub limits: LimitsConfig,
    /// Security and authentication settings (defaults applied if not specified)
    #[serde(default)]
    pub security: SecurityConfig,
}

/// Server configuration settings.
///
/// Defines the basic network configuration for the HTTP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Default host/IP address to bind to (e.g., "0.0.0.0" or "localhost")
    pub default_host: String,
    /// Default port number to listen on
    pub default_port: u16,
    /// URL scheme ("http" or "https")
    pub scheme: String,
}

/// Storage configuration for package files.
///
/// Defines the directory structure used to store different package types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Base directory for all package storage
    pub default_data_dir: PathBuf,
    /// Subdirectory structure for different package types
    pub directories: StorageDirectories,
}

/// Directory structure for different package registry types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageDirectories {
    /// PyPI package storage configuration
    pub pypi: PypiStorage,
    /// npm package storage configuration
    pub npm: NpmStorage,
    /// Cargo crate storage configuration
    pub cargo: CargoStorage,
}

/// PyPI-specific storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypiStorage {
    /// Directory path for storing PyPI package files
    pub packages: PathBuf,
}

/// npm-specific storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmStorage {
    /// Directory path for storing npm package tarballs
    pub tarballs: PathBuf,
    /// Directory path for storing npm package metadata
    pub metadata: PathBuf,
}

/// Cargo-specific storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoStorage {
    /// Directory path for storing crate files
    pub crates: PathBuf,
    /// Directory path for storing crate index data
    pub index: PathBuf,
}

/// URL route configuration for different package types.
///
/// Defines the URL patterns used for API endpoints across all supported package types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConfig {
    /// PyPI-specific route patterns
    pub pypi: PypiRoutes,
    /// npm-specific route patterns
    pub npm: NpmRoutes,
    /// Cargo-specific route patterns
    pub cargo: CargoRoutes,
    /// Web UI route patterns
    pub ui: UiRoutes,
}

/// Route patterns for PyPI package operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypiRoutes {
    /// URL pattern for PyPI simple index (package listing)
    pub simple_index: String,
    /// URL pattern for individual package index pages
    pub package_index: String,
    /// URL pattern for package downloads
    pub download: String,
    /// URL pattern for package uploads
    pub upload: String,
}

/// Route patterns for npm package operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmRoutes {
    /// URL pattern for npm package metadata
    pub metadata: String,
    /// URL pattern for npm package downloads
    pub download: String,
    /// URL pattern for npm package publishing
    pub publish: String,
}

/// Route patterns for Cargo crate operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoRoutes {
    /// URL pattern for Cargo configuration endpoint
    pub config: String,
    /// URL pattern for crate downloads
    pub download: String,
    /// URL pattern for crate publishing
    pub publish: String,
    /// URL pattern for crate index access
    pub index: String,
}

/// Route patterns for web UI pages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiRoutes {
    /// URL pattern for the home page
    pub home: String,
    /// URL pattern for package listing pages
    pub list: String,
    /// URL pattern for PyPI package detail pages
    pub pypi_detail: String,
    /// URL pattern for npm package detail pages
    pub npm_detail: String,
    /// URL pattern for Cargo crate detail pages
    pub cargo_detail: String,
}

/// Configuration templates for package manager clients.
///
/// Contains configuration settings that can be used to generate
/// client configuration files for various package managers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// npm client configuration
    pub npm: NpmClientConfig,
    /// Cargo client configuration
    pub cargo: CargoClientConfig,
    /// pip client configuration
    pub pip: PipClientConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmClientConfig {
    pub config_file: String,
    pub backup_file: String,
    pub registry_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoClientConfig {
    pub config_dir: PathBuf,
    pub config_file: String,
    pub backup_file: String,
    pub registry_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipClientConfig {
    pub config_dir_unix: PathBuf,
    pub config_dir_windows: PathBuf,
    pub config_file_unix: String,
    pub config_file_windows: String,
    pub backup_suffix: String,
    pub index_format: String,
}

/// External service URLs.
///
/// Defines URLs for external services and fallback registries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalUrls {
    /// Fallback PyPI registry URL when packages are not found locally
    pub pypi_fallback: String,
}

/// Upload and request limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    pub max_upload_size_mb: usize,
    pub max_request_body_size_mb: usize,
    pub rate_limit: RateLimitConfig,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        LimitsConfig {
            max_upload_size_mb: 100,
            max_request_body_size_mb: 150,
            rate_limit: RateLimitConfig::default(),
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            enabled: false,
            requests_per_minute: 60,
            burst_size: 100,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    pub require_authentication: bool,
    pub api_keys: Vec<String>,
    pub allowed_publishers: Vec<String>,
}

impl Config {
    /// Load configuration from a JSON file.
    ///
    /// Reads and parses a JSON configuration file from the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the JSON configuration file
    ///
    /// # Returns
    ///
    /// * `Ok(Config)` if the file was successfully loaded and parsed
    /// * `Err(AppError)` if the file cannot be read or contains invalid JSON
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file cannot be read (file not found, permissions, etc.)
    /// - The file contains invalid JSON
    /// - The JSON structure doesn't match the expected configuration format
    pub fn load<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let config_str = fs::read_to_string(path)?;
        let config = serde_json::from_str(&config_str)?;
        Ok(config)
    }

    /// Get the maximum upload size in bytes.
    ///
    /// Converts the configured maximum upload size from megabytes to bytes.
    ///
    /// # Returns
    ///
    /// The maximum upload size in bytes
    pub fn max_upload_size_bytes(&self) -> usize {
        self.limits.max_upload_size_mb * 1024 * 1024
    }

    /// Get the maximum request body size in bytes.
    ///
    /// Converts the configured maximum request body size from megabytes to bytes.
    ///
    /// # Returns
    ///
    /// The maximum request body size in bytes
    pub fn max_request_body_size_bytes(&self) -> usize {
        self.limits.max_request_body_size_mb * 1024 * 1024
    }

    /// Load configuration from file with fallback to defaults.
    ///
    /// Attempts to load configuration from the specified path. If the file
    /// doesn't exist, returns the default configuration instead. This is
    /// useful for optional configuration files.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the JSON configuration file
    ///
    /// # Returns
    ///
    /// * `Ok(Config)` with either loaded or default configuration
    /// * `Err(AppError)` if the file exists but cannot be parsed
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file exists but cannot be read
    /// - The file exists but contains invalid JSON
    /// - The JSON structure doesn't match the expected configuration format
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        if path.as_ref().exists() {
            Self::load(path)
        } else {
            Ok(Self::default())
        }
    }
}

/// Default routes - these are not configurable as they follow package manager standards
fn default_routes() -> RouteConfig {
    RouteConfig {
        pypi: PypiRoutes {
            simple_index: "/pypi/simple/".to_string(),
            package_index: "/pypi/simple/{package}/".to_string(),
            download: "/pypi/packages/{filename}".to_string(),
            upload: "/pypi/".to_string(),
        },
        npm: NpmRoutes {
            metadata: "/npm/{package}".to_string(),
            download: "/npm/{package}/-/{filename}".to_string(),
            publish: "/npm/{package}".to_string(),
        },
        cargo: CargoRoutes {
            config: "/cargo/config.json".to_string(),
            download: "/cargo/api/v1/crates/{crate}/{version}/download".to_string(),
            publish: "/cargo/api/v1/crates/new".to_string(),
            index: "/cargo/index/{path}".to_string(),
        },
        ui: UiRoutes {
            home: "/".to_string(),
            list: "/packages".to_string(),
            pypi_detail: "/pypi/{package}".to_string(),
            npm_detail: "/npm/{package}".to_string(),
            cargo_detail: "/cargo/{crate}".to_string(),
        },
    }
}

/// Default client configuration - these are standard formats for package managers
fn default_client_config() -> ClientConfig {
    ClientConfig {
        npm: NpmClientConfig {
            config_file: ".npmrc".to_string(),
            backup_file: ".npmrc.backup".to_string(),
            registry_format: "registry={server_url}/npm/".to_string(),
        },
        cargo: CargoClientConfig {
            config_dir: PathBuf::from(".cargo"),
            config_file: "config.toml".to_string(),
            backup_file: "config.toml.backup".to_string(),
            registry_format: "[registries.local]\nindex = \"{server_url}/cargo/index\"".to_string(),
        },
        pip: PipClientConfig {
            config_dir_unix: PathBuf::from(".config/pip"),
            config_dir_windows: PathBuf::from("AppData/Roaming/pip"),
            config_file_unix: "pip.conf".to_string(),
            config_file_windows: "pip.ini".to_string(),
            backup_suffix: ".backup".to_string(),
            index_format: "[global]\nindex-url = {server_url}/pypi/simple/".to_string(),
        },
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut config: Config = serde_json::from_str(include_str!("../config.json"))
            .expect("Failed to parse embedded config.json");

        // Add hardcoded defaults for routes and client config
        config.routes = default_routes();
        config.client_config = default_client_config();

        config
    }
}
