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
mod client_ops;
mod crypto;
mod server;
mod storage;
mod types;

pub use client_ops::{
    add_secret, get_secret_for_vm, get_secret_value, list_secrets, remove_secret,
};
pub use server::{check_server_running, run_server_with_shutdown};
pub use storage::get_auth_data_dir;
pub use types::{SecretListResponse, SecretScope, SecretSummary};
