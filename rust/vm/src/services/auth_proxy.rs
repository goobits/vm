//! Auth Proxy Service Implementation

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::sync::oneshot::Sender;
use tracing::{info, warn};
use vm_config::GlobalConfig;

use super::ManagedService;
use crate::error::VmError;

/// Auth Proxy service that implements the ManagedService trait
pub struct AuthProxyService {
    /// Shutdown handles for graceful shutdown
    shutdown_handles: Arc<Mutex<HashMap<String, Sender<()>>>>,
}

impl AuthProxyService {
    /// Create a new AuthProxyService instance
    pub fn new(shutdown_handles: Arc<Mutex<HashMap<String, Sender<()>>>>) -> Self {
        Self { shutdown_handles }
    }
}

#[async_trait::async_trait]
impl ManagedService for AuthProxyService {
    async fn start(&self, global_config: &GlobalConfig) -> Result<()> {
        let port = self.get_port(global_config);

        use vm_auth_proxy;

        let data_dir = vm_auth_proxy::storage::get_auth_data_dir()
            .context("Failed to get auth data directory")?;

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        // Store shutdown handle for later use
        {
            let mut handles = self.shutdown_handles.lock().map_err(|e| {
                VmError::general(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    "Shutdown handles mutex was poisoned",
                )
            })?;
            handles.insert("auth_proxy".to_string(), shutdown_tx);
        }

        // Spawn server with shutdown capability
        tokio::spawn(async move {
            if let Err(e) = vm_auth_proxy::run_server_with_shutdown(
                "127.0.0.1".to_string(),
                port,
                data_dir,
                Some(shutdown_rx),
            )
            .await
            {
                warn!("Auth proxy exited with error: {}", e);
            }
        });

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::debug!("Auth proxy stop requested");

        // Get shutdown handle
        let shutdown_tx = {
            let mut handles = self.shutdown_handles.lock().map_err(|e| {
                VmError::general(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    "Shutdown handles mutex was poisoned",
                )
            })?;
            handles.remove("auth_proxy")
        };

        if let Some(shutdown_tx) = shutdown_tx {
            // Send shutdown signal
            if shutdown_tx.send(()).is_err() {
                warn!(
                    "Failed to send shutdown signal to auth proxy (receiver may have been dropped)"
                );
            } else {
                info!("Shutdown signal sent to auth proxy");

                // Give the server a brief moment to shut down gracefully
                // Reduced from 1000ms to 200ms for faster stops
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
        } else {
            warn!("No shutdown handle found for auth proxy - it may not be running or was started externally");
        }

        Ok(())
    }

    async fn check_health(&self, global_config: &GlobalConfig) -> bool {
        let port = self.get_port(global_config);
        let endpoint = format!("http://localhost:{port}/health");

        // Use reqwest to check health for HTTP-based services
        match reqwest::get(&endpoint).await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    fn name(&self) -> &str {
        "auth_proxy"
    }

    fn get_port(&self, global_config: &GlobalConfig) -> u16 {
        global_config.services.auth_proxy.port
    }
}
