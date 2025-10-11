//! Docker daemon configuration management
//!
//! This module provides transparent configuration of the host Docker daemon
//! to use the local registry as a mirror, enabling seamless caching.

use crate::types::DaemonConfig;
use anyhow::{anyhow, Result};
use serde_json;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Docker daemon configuration manager
pub struct DockerConfigManager {
    daemon_json_path: PathBuf,
    backup_path: PathBuf,
}

impl DockerConfigManager {
    /// Create new Docker config manager
    pub fn new() -> Result<Self> {
        let daemon_json_path = find_docker_daemon_json()?;
        let backup_path = daemon_json_path.with_extension("json.vm-backup");

        Ok(Self {
            daemon_json_path,
            backup_path,
        })
    }

    /// Configure Docker daemon to use local registry as mirror
    pub async fn configure_docker_daemon(&self, registry_url: &str) -> Result<()> {
        debug!(
            "Configuring Docker daemon to use registry mirror: {}",
            registry_url
        );

        // Read existing daemon.json if it exists
        let mut config = if self.daemon_json_path.exists() {
            self.load_existing_config().await?
        } else {
            DaemonConfig {
                registry_mirrors: vec![],
                insecure_registries: vec![],
            }
        };

        // Create backup if config file exists and we haven't backed it up yet
        if self.daemon_json_path.exists() && !self.backup_path.exists() {
            self.create_backup().await?;
        }

        // Add our registry if not already present
        if !config.registry_mirrors.contains(&registry_url.to_string()) {
            config.registry_mirrors.insert(0, registry_url.to_string());
        }

        if !config
            .insecure_registries
            .contains(&registry_url.to_string())
        {
            config.insecure_registries.push(registry_url.to_string());
        }

        // Write updated config
        self.write_config(&config).await?;

        // Restart Docker daemon to pick up changes
        self.restart_docker_daemon().await?;

        info!("Docker daemon configured to use local registry mirror");
        Ok(())
    }

    /// Remove local registry configuration from Docker daemon
    pub async fn unconfigure_docker_daemon(&self, registry_url: &str) -> Result<()> {
        debug!("Removing registry mirror configuration: {}", registry_url);

        if !self.daemon_json_path.exists() {
            debug!("No daemon.json exists, nothing to unconfigure");
            return Ok(());
        }

        let mut config = self.load_existing_config().await?;

        // Remove our registry
        config.registry_mirrors.retain(|url| url != registry_url);
        config.insecure_registries.retain(|url| url != registry_url);

        // If we have a backup and config is now empty, restore backup
        if config.registry_mirrors.is_empty()
            && config.insecure_registries.is_empty()
            && self.backup_path.exists()
        {
            self.restore_backup().await?;
        } else {
            self.write_config(&config).await?;
        }

        // Restart Docker daemon
        self.restart_docker_daemon().await?;

        info!("Docker daemon registry mirror configuration removed");
        Ok(())
    }

    /// Load existing daemon.json configuration
    async fn load_existing_config(&self) -> Result<DaemonConfig> {
        let content = fs::read_to_string(&self.daemon_json_path)
            .map_err(|e| anyhow!("Failed to read daemon.json: {e}"))?;

        if content.trim().is_empty() {
            return Ok(DaemonConfig {
                registry_mirrors: vec![],
                insecure_registries: vec![],
            });
        }

        // Parse existing config - handle partial configs gracefully
        let config: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse daemon.json: {e}"))?;

        let registry_mirrors = config
            .get("registry-mirrors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let insecure_registries = config
            .get("insecure-registries")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(DaemonConfig {
            registry_mirrors,
            insecure_registries,
        })
    }

    /// Write daemon configuration to file
    async fn write_config(&self, config: &DaemonConfig) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.daemon_json_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| anyhow!("Failed to create Docker config directory: {e}"))?;
        }

        let json = serde_json::to_string_pretty(config)
            .map_err(|e| anyhow!("Failed to serialize daemon config: {e}"))?;

        fs::write(&self.daemon_json_path, json)
            .map_err(|e| anyhow!("Failed to write daemon.json: {e}"))?;

        debug!(
            "Docker daemon configuration written to {:?}",
            self.daemon_json_path
        );
        Ok(())
    }

    /// Create backup of existing daemon.json
    async fn create_backup(&self) -> Result<()> {
        fs::copy(&self.daemon_json_path, &self.backup_path)
            .map_err(|e| anyhow!("Failed to create backup of daemon.json: {e}"))?;

        debug!("Created backup of daemon.json at {:?}", self.backup_path);
        Ok(())
    }

    /// Restore backup of daemon.json
    async fn restore_backup(&self) -> Result<()> {
        if !self.backup_path.exists() {
            return Err(anyhow!("No backup file found to restore"));
        }

        fs::copy(&self.backup_path, &self.daemon_json_path)
            .map_err(|e| anyhow!("Failed to restore daemon.json backup: {e}"))?;

        // Remove backup file
        fs::remove_file(&self.backup_path)
            .map_err(|e| anyhow!("Failed to remove backup file: {e}"))?;

        debug!("Restored daemon.json from backup");
        Ok(())
    }

    /// Restart Docker daemon to pick up configuration changes
    async fn restart_docker_daemon(&self) -> Result<()> {
        debug!("Attempting to restart Docker daemon");

        // Try different methods based on the system
        let restart_commands = vec![
            "systemctl restart docker",
            "service docker restart",
            "brew services restart docker",
            "pkill -HUP dockerd", // Send HUP signal to reload config
        ];

        for cmd in restart_commands {
            debug!("Trying restart command: {}", cmd);

            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .await;

            match output {
                Ok(output) if output.status.success() => {
                    info!("Docker daemon restart successful with: {}", cmd);

                    // Wait a moment for Docker to fully restart
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                    return Ok(());
                }
                Ok(output) => {
                    debug!("Command '{}' failed with status: {}", cmd, output.status);
                }
                Err(e) => {
                    debug!("Command '{}' failed to execute: {}", cmd, e);
                }
            }
        }

        warn!("Could not restart Docker daemon automatically. Registry configuration updated but may require manual Docker restart.");
        Ok(())
    }

    /// Check if Docker daemon is currently configured with our registry
    pub async fn is_configured(&self, registry_url: &str) -> Result<bool> {
        if !self.daemon_json_path.exists() {
            return Ok(false);
        }

        let config = self.load_existing_config().await?;
        Ok(config.registry_mirrors.contains(&registry_url.to_string()))
    }
}

impl Default for DockerConfigManager {
    fn default() -> Self {
        Self::new().expect("Failed to create DockerConfigManager")
    }
}

/// Find the Docker daemon.json file location
fn find_docker_daemon_json() -> Result<PathBuf> {
    // Common locations for daemon.json
    let potential_paths = vec![
        "/etc/docker/daemon.json",                      // Linux system-wide
        "~/.docker/daemon.json",                        // User-specific
        "/usr/local/etc/docker/daemon.json",            // macOS Homebrew
        "C:\\ProgramData\\Docker\\config\\daemon.json", // Windows
    ];

    for path_str in potential_paths {
        let path = if path_str.starts_with('~') {
            dirs::home_dir()
                .ok_or_else(|| anyhow!("Could not find home directory"))?
                .join(path_str.strip_prefix("~/").unwrap_or(path_str))
        } else {
            PathBuf::from(path_str)
        };

        // Return the first existing parent directory, or the first likely location
        if let Some(parent) = path.parent() {
            if parent.exists() {
                debug!("Using Docker daemon.json location: {:?}", path);
                return Ok(path);
            }
        }
    }

    // If no existing parent found, default to the most common location
    let default_path = PathBuf::from("/etc/docker/daemon.json");
    debug!(
        "No existing Docker config directory found, using default: {:?}",
        default_path
    );
    Ok(default_path)
}

/// Configure Docker daemon to use local registry (convenience function)
pub async fn configure_docker_daemon(registry_url: &str) -> Result<()> {
    let manager = DockerConfigManager::new()?;
    manager.configure_docker_daemon(registry_url).await
}

/// Remove Docker daemon configuration for local registry (convenience function)
pub async fn unconfigure_docker_daemon(registry_url: &str) -> Result<()> {
    let manager = DockerConfigManager::new()?;
    manager.unconfigure_docker_daemon(registry_url).await
}

/// Check if Docker daemon is configured with local registry (convenience function)
pub async fn is_docker_configured(registry_url: &str) -> Result<bool> {
    let manager = DockerConfigManager::new()?;
    manager.is_configured(registry_url).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_docker_daemon_json() {
        // This test just ensures the function doesn't panic
        let path = find_docker_daemon_json();
        assert!(path.is_ok());
    }

    #[tokio::test]
    async fn test_daemon_config_serialization() {
        let config = DaemonConfig::for_registry("http://localhost:5000");
        let json = serde_json::to_string_pretty(&config).unwrap();

        assert!(json.contains("registry-mirrors"));
        assert!(json.contains("insecure-registries"));
        assert!(json.contains("http://localhost:5000"));
    }

    #[tokio::test]
    async fn test_config_manager_with_temp_file() {
        let temp_dir = TempDir::new().unwrap();
        let daemon_json_path = temp_dir.path().join("daemon.json");
        let backup_path = daemon_json_path.with_extension("json.vm-backup");

        let manager = DockerConfigManager {
            daemon_json_path,
            backup_path,
        };

        // Test writing and reading config
        let config = DaemonConfig::for_registry("http://localhost:5000");
        manager.write_config(&config).await.unwrap();

        let loaded_config = manager.load_existing_config().await.unwrap();
        assert_eq!(
            loaded_config.registry_mirrors,
            vec!["http://localhost:5000"]
        );
        assert_eq!(
            loaded_config.insecure_registries,
            vec!["http://localhost:5000"]
        );
    }
}
