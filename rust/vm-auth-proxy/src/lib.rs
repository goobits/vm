//! # VM Auth Proxy
//!
//! A centralized secrets management service for secure credential sharing across VMs.
//! Provides encrypted storage and HTTP API for managing secrets with automatic
//! environment variable injection into VMs.
//!
//! ## Features
//!
//! - **AES-256-GCM encryption**: Secure storage of API keys and credentials
//! - **HTTP API**: RESTful interface for secret management (port 3090)
//! - **VM integration**: Automatic environment variable injection
//! - **Bearer token auth**: Secure communication between VMs and host
//! - **Audit logging**: Track secret access and modifications
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vm_auth_proxy::server;
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Start auth proxy server
//! server::run_server("127.0.0.1".to_string(), 3090, PathBuf::from("~/.vm/auth")).await?;
//! # Ok(())
//! # }
//! ```

pub mod client_ops;
pub mod crypto;
pub mod server;
pub mod storage;
pub mod types;

// Re-export main types
pub use types::{Secret, SecretScope, SecretStorage};

// Re-export server functions
pub use server::{run_server, run_server_background};

// Re-export client operations for CLI
pub use client_ops::{
    add_secret, check_server_running, get_secret_for_vm, list_secrets, remove_secret,
};

/// Default port for the auth proxy service
pub const DEFAULT_PORT: u16 = 3090;

/// Default host for the auth proxy service
pub const DEFAULT_HOST: &str = "127.0.0.1";

/// Auth proxy service name for logging and process management
pub const SERVICE_NAME: &str = "vm-auth-proxy";
