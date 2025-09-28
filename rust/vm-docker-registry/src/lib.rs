//! # VM Docker Registry
//!
//! A local Docker registry service for caching images and eliminating redundant downloads.
//! Uses nginx proxy with registry:2 backend for true pull-through caching functionality.
//!
//! ## Features
//!
//! - **Pull-through caching**: First pull from Docker Hub, subsequent pulls from local cache
//! - **Bandwidth savings**: 80-95% reduction for teams reusing base images
//! - **Offline capability**: Previously pulled images available without internet
//! - **Automatic cleanup**: Configurable garbage collection and size limits
//! - **VM integration**: Automatic Docker daemon configuration
//!
//! ## Architecture
//!
//! ```text
//! VM Docker Client → nginx proxy (port 5000) → registry:2 backend (port 5001)
//!                              ↓ (on cache miss)
//!                         Docker Hub
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vm_docker_registry::server;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Start Docker registry with default settings
//! server::start_registry().await?;
//! # Ok(())
//! # }
//! ```

pub mod auto_manager;
pub mod config;
pub mod docker_config;
pub mod server;
pub mod types;

// Re-export main types
pub use types::{AutoConfig, ContainerInfo, RegistryConfig, RegistryStatus};

// Re-export server functions
pub use server::{check_registry_running, start_registry, stop_registry};

// Re-export auto-manager functions
pub use auto_manager::{start_auto_manager, start_auto_manager_with_config};

// Re-export docker configuration functions
pub use docker_config::{configure_docker_daemon, is_docker_configured, unconfigure_docker_daemon};

/// Default port for the Docker registry proxy
pub const DEFAULT_REGISTRY_PORT: u16 = 5000;

/// Default port for the registry backend
pub const DEFAULT_BACKEND_PORT: u16 = 5001;

/// Default host for the Docker registry
pub const DEFAULT_HOST: &str = "127.0.0.1";

/// Registry service name for logging and process management
pub const SERVICE_NAME: &str = "vm-docker-registry";

/// Container names
pub const PROXY_CONTAINER_NAME: &str = "vm-registry-proxy";
pub const BACKEND_CONTAINER_NAME: &str = "vm-registry-backend";
