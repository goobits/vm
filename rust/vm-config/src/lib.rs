//! VM configuration management library.
//!
//! This library provides functionality for managing VM configurations, including
//! loading, merging, validating configurations, and managing presets.
//!
//! ## Main Features
//! - Configuration loading and validation
//! - Configuration merging and preset management
//! - CLI utilities for configuration initialization
//! - Resource management utilities

pub mod cli;
pub mod config;
mod config_ops;
pub mod detector;
mod embedded_presets;
pub mod global_config;
pub mod merge;
mod paths; // Internal only
pub mod ports;
mod preset; // Internal only
pub mod resources; // VM resource suggestions
pub mod validate;
pub mod yaml; // YAML operations module

#[cfg(test)]
mod test_memory;

// Re-export commonly needed path utilities
pub use paths::{get_current_gid, get_current_uid, get_tool_dir, resolve_tool_path};

// Re-export config operations for use by main vm binary
pub use config_ops::{load_global_config, ConfigOps};

// Re-export CLI functions for direct use
pub use cli::init_config_file;

// Re-export global config for use by other crates
pub use global_config::GlobalConfig;

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Complete application configuration containing both global and VM-specific settings
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Global configuration from ~/.vm/config.yaml
    pub global: GlobalConfig,
    /// VM-specific configuration from vm.yaml
    pub vm: config::VmConfig,
}

impl AppConfig {
    /// Load complete configuration from standard locations
    ///
    /// This is the main entry point for loading all configuration. It:
    /// 1. Loads global config from ~/.vm/config.yaml
    /// 2. Loads VM config from provided path or auto-discovers
    /// 3. Applies defaults and presets
    /// 4. Merges configurations in proper precedence order
    /// 5. Handles backward compatibility for deprecated fields
    pub fn load(config_path: Option<PathBuf>) -> Result<Self> {
        // Load global configuration
        let global = GlobalConfig::load().context("Failed to load global configuration")?;

        // Load VM configuration with all merging logic
        let vm = config::VmConfig::load(config_path.clone())?;

        // Handle backward compatibility: if deprecated fields are in vm.yaml,
        // warn and apply them to the runtime global config (but don't save)
        let mut runtime_global = global.clone();
        Self::handle_deprecated_fields_raw(config_path, &mut runtime_global)?;

        Ok(Self {
            global: runtime_global,
            vm,
        })
    }

    /// Handle deprecated fields for backward compatibility
    pub fn handle_deprecated_fields_raw(
        vm_yaml_path: Option<PathBuf>,
        global: &mut GlobalConfig,
    ) -> Result<()> {
        use std::collections::HashMap;
        use tracing::warn;

        // Read the raw YAML to check for deprecated fields
        let yaml_path = match vm_yaml_path {
            Some(path) => path,
            None => {
                // Try to find vm.yaml in current directory
                let current_dir =
                    std::env::current_dir().context("Failed to get current directory")?;
                current_dir.join("vm.yaml")
            }
        };

        if !yaml_path.exists() {
            return Ok(()); // No file, no deprecated fields
        }

        let yaml_content = std::fs::read_to_string(&yaml_path)
            .with_context(|| format!("Failed to read {:?}", yaml_path))?;

        // Parse as raw YAML to check for deprecated fields
        let raw_yaml: HashMap<String, serde_yaml_ng::Value> =
            serde_yaml_ng::from_str(&yaml_content)
                .context("Failed to parse YAML for deprecated field detection")?;

        // Check for deprecated fields and warn
        if let Some(serde_yaml_ng::Value::Bool(true)) = raw_yaml.get("docker_registry") {
            warn!(
                "The 'docker_registry' field in vm.yaml is deprecated. \
                Please move it to ~/.vm/config.yaml under 'services.docker_registry.enabled'"
            );
            global.services.docker_registry.enabled = true;
        }

        if let Some(serde_yaml_ng::Value::Bool(true)) = raw_yaml.get("auth_proxy") {
            warn!(
                "The 'auth_proxy' field in vm.yaml is deprecated. \
                Please move it to ~/.vm/config.yaml under 'services.auth_proxy.enabled'"
            );
            global.services.auth_proxy.enabled = true;
        }

        if let Some(serde_yaml_ng::Value::Bool(true)) = raw_yaml.get("package_registry") {
            warn!(
                "The 'package_registry' field in vm.yaml is deprecated. \
                Please move it to ~/.vm/config.yaml under 'services.package_registry.enabled'"
            );
            global.services.package_registry.enabled = true;
        }

        Ok(())
    }
}
