//! User configuration management
//!
//! Handles persistent user configuration for the package server,
//! stored in the user's home directory.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// User configuration stored in ~/.config/goobits-pkg-server/config.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserConfig {
    /// Server settings
    #[serde(default)]
    pub server: ServerSettings,
    /// Registry settings
    #[serde(default)]
    pub registry: RegistrySettings,
    /// Client behavior settings
    #[serde(default)]
    pub client: ClientSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    /// Default port to use
    pub port: u16,
    /// Default host to bind to
    pub host: String,
    /// Default data directory
    pub data_dir: PathBuf,
    /// Auto-start server if not running when using pkg-server add
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySettings {
    /// Enable fallback to upstream PyPI
    pub pypi_fallback: bool,
    /// Enable fallback to upstream npm
    pub npm_fallback: bool,
    /// Enable fallback to upstream crates.io
    pub cargo_fallback: bool,
    /// Cache TTL in seconds for upstream packages
    pub cache_ttl: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientSettings {
    /// Auto-configure package managers on server start
    pub auto_configure: bool,
    /// Default shell for pkg-server use command
    pub default_shell: Option<String>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            port: 3080,
            host: "0.0.0.0".to_string(),
            data_dir: PathBuf::from("./data"),
            auto_start: false,
        }
    }
}

impl Default for RegistrySettings {
    fn default() -> Self {
        Self {
            pypi_fallback: true,
            npm_fallback: true,
            cargo_fallback: true,
            cache_ttl: 3600, // 1 hour
        }
    }
}

impl UserConfig {
    /// Get the configuration file path
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home
            .join(".config")
            .join("goobits-pkg-server")
            .join("config.toml"))
    }

    /// Load configuration from disk or create default
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config from {:?}", path))?;
            toml::from_str(&content)
                .with_context(|| format!("Failed to parse config from {:?}", path))
        } else {
            Ok(Self::default())
        }
    }

    /// Save configuration to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize configuration")?;

        fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {:?}", path))?;

        Ok(())
    }

    /// Get a configuration value by key
    pub fn get(&self, key: &str) -> Result<String> {
        match key {
            "port" => Ok(self.server.port.to_string()),
            "host" => Ok(self.server.host.clone()),
            "data_dir" => Ok(self.server.data_dir.display().to_string()),
            "auto_start" => Ok(self.server.auto_start.to_string()),
            "pypi_fallback" => Ok(self.registry.pypi_fallback.to_string()),
            "npm_fallback" => Ok(self.registry.npm_fallback.to_string()),
            "cargo_fallback" => Ok(self.registry.cargo_fallback.to_string()),
            "cache_ttl" => Ok(self.registry.cache_ttl.to_string()),
            "auto_configure" => Ok(self.client.auto_configure.to_string()),
            "default_shell" => Ok(self
                .client
                .default_shell
                .clone()
                .unwrap_or_else(|| "none".to_string())),
            _ => anyhow::bail!("Unknown configuration key: {}", key),
        }
    }

    /// Set a configuration value by key
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "port" => {
                self.server.port = value
                    .parse()
                    .with_context(|| format!("Invalid port value: {}", value))?;
            }
            "host" => {
                self.server.host = value.to_string();
            }
            "data_dir" => {
                self.server.data_dir = PathBuf::from(value);
            }
            "auto_start" => {
                self.server.auto_start = value
                    .parse()
                    .with_context(|| format!("Invalid boolean value: {}", value))?;
            }
            "pypi_fallback" => {
                self.registry.pypi_fallback = value
                    .parse()
                    .with_context(|| format!("Invalid boolean value: {}", value))?;
            }
            "npm_fallback" => {
                self.registry.npm_fallback = value
                    .parse()
                    .with_context(|| format!("Invalid boolean value: {}", value))?;
            }
            "cargo_fallback" => {
                self.registry.cargo_fallback = value
                    .parse()
                    .with_context(|| format!("Invalid boolean value: {}", value))?;
            }
            "cache_ttl" => {
                self.registry.cache_ttl = value
                    .parse()
                    .with_context(|| format!("Invalid cache_ttl value: {}", value))?;
            }
            "auto_configure" => {
                self.client.auto_configure = value
                    .parse()
                    .with_context(|| format!("Invalid boolean value: {}", value))?;
            }
            "default_shell" => {
                self.client.default_shell = if value == "none" {
                    None
                } else {
                    Some(value.to_string())
                };
            }
            _ => anyhow::bail!("Unknown configuration key: {}", key),
        }
        Ok(())
    }

    /// Display all configuration values
    pub fn display(&self) -> String {
        format!(
            r#"📦 Goobits Package Server Configuration

Server Settings:
  port: {}
  host: {}
  data_dir: {}
  auto_start: {}

Registry Settings:
  pypi_fallback: {}
  npm_fallback: {}
  cargo_fallback: {}
  cache_ttl: {}s

Client Settings:
  auto_configure: {}
  default_shell: {}

Config file: {}
"#,
            self.server.port,
            self.server.host,
            self.server.data_dir.display(),
            self.server.auto_start,
            self.registry.pypi_fallback,
            self.registry.npm_fallback,
            self.registry.cargo_fallback,
            self.registry.cache_ttl,
            self.client.auto_configure,
            self.client
                .default_shell
                .as_ref()
                .unwrap_or(&"auto-detect".to_string()),
            Self::config_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        )
    }

    /// Get available configuration keys
    pub fn available_keys() -> &'static [&'static str] {
        &[
            "port",
            "host",
            "data_dir",
            "auto_start",
            "pypi_fallback",
            "npm_fallback",
            "cargo_fallback",
            "cache_ttl",
            "auto_configure",
            "default_shell",
        ]
    }
}
