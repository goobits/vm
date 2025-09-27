//! # Application State Management
//!
//! This module defines the shared application state used throughout the package registry server.
//! It contains the core data structures that hold runtime configuration and shared resources.
//!
//! ## Key Types
//!
//! - [`AppState`]: The main application state containing shared configuration and client instances
//! - [`SuccessResponse`]: Standardized success response format for API endpoints
//!
//! ## Usage
//!
//! The [`AppState`] is typically created during server initialization and shared across
//! all request handlers using Arc (atomic reference counting) for thread-safe access.
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use crate::state::AppState;
//! use crate::upstream::{UpstreamClient, UpstreamConfig};
//!
//! let upstream_config = UpstreamConfig::default();
//! let upstream_client = Arc::new(UpstreamClient::new(upstream_config)?);
//!
//! let state = Arc::new(AppState {
//!     data_dir: "/path/to/data".into(),
//!     server_addr: "http://localhost:3080".to_string(),
//!     upstream_client,
//! });
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::config::Config;
use crate::upstream::UpstreamClient;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;

/// Application state containing shared configuration and resources.
///
/// This struct holds the core runtime state that needs to be shared across
/// all HTTP request handlers. It includes storage paths, server configuration,
/// and upstream registry clients.
///
/// # Thread Safety
///
/// This struct is designed to be wrapped in an `Arc` and shared across multiple
/// threads safely. The `UpstreamClient` is already wrapped in an `Arc` internally.
///
/// # Fields
///
/// * `data_dir` - Base directory for all package storage
/// * `server_addr` - Full server address (scheme://host:port) used for generating URLs
/// * `upstream_client` - HTTP client for communicating with upstream registries
/// * `config` - Application configuration including security settings
#[derive(Clone)]
pub struct AppState {
    /// Base directory path where all package files are stored
    pub data_dir: PathBuf,
    /// Full server address including scheme, host, and port (e.g., "http://localhost:3080")
    pub server_addr: String,
    /// Shared HTTP client for upstream registry communication
    pub upstream_client: Arc<UpstreamClient>,
    /// Application configuration
    pub config: Arc<Config>,
}

/// Standardized success response for API consistency.
///
/// This struct provides a uniform format for successful API responses,
/// ensuring consistency across all endpoints that don't return specific data.
///
/// # JSON Format
///
/// Serializes to: `{"message": "Operation completed successfully"}`
#[derive(Serialize)]
pub struct SuccessResponse {
    /// Human-readable success message describing the completed operation
    pub message: String,
}
