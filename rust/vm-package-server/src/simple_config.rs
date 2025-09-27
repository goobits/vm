//! Simplified configuration for VM tool integration
//!
//! This module provides a simplified configuration interface for the VM tool,
//! replacing the complex user configuration system with sensible defaults.

use std::path::PathBuf;

/// Simplified configuration for package registry
#[derive(Debug, Clone)]
pub struct SimpleConfig {
    pub port: u16,
    pub host: String,
    pub data_dir: PathBuf,
    pub fallback_enabled: bool,
}

impl Default for SimpleConfig {
    fn default() -> Self {
        Self {
            port: 3080,
            host: "0.0.0.0".to_string(),
            data_dir: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".vm")
                .join("pkg-server"),
            fallback_enabled: true,
        }
    }
}

impl SimpleConfig {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            ..Default::default()
        }
    }

    pub fn with_host(mut self, host: String) -> Self {
        self.host = host;
        self
    }

    pub fn with_data_dir(mut self, data_dir: PathBuf) -> Self {
        self.data_dir = data_dir;
        self
    }

    pub fn with_fallback(mut self, enabled: bool) -> Self {
        self.fallback_enabled = enabled;
        self
    }
}
