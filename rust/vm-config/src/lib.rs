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
        let mut vm = config::VmConfig::load(config_path)?;

        // Handle backward compatibility: if deprecated fields are in vm.yaml,
        // warn and apply them to the runtime global config (but don't save)
        let mut runtime_global = global.clone();
        Self::handle_deprecated_fields(&mut vm, &mut runtime_global)?;

        Ok(Self {
            global: runtime_global,
            vm,
        })
    }

    /// Handle deprecated fields for backward compatibility
    fn handle_deprecated_fields(
        vm: &mut config::VmConfig,
        global: &mut GlobalConfig,
    ) -> Result<()> {
        use tracing::warn;

        // Check for deprecated docker_registry field
        if vm.docker_registry {
            warn!(
                "The 'docker_registry' field in vm.yaml is deprecated. \
                Please move it to ~/.vm/config.yaml under 'services.docker_registry.enabled'"
            );
            global.services.docker_registry.enabled = true;
            vm.docker_registry = false;
        }

        // Check for deprecated auth_proxy field
        if vm.auth_proxy {
            warn!(
                "The 'auth_proxy' field in vm.yaml is deprecated. \
                Please move it to ~/.vm/config.yaml under 'services.auth_proxy.enabled'"
            );
            global.services.auth_proxy.enabled = true;
            vm.auth_proxy = false;
        }

        // Check for deprecated package_registry field
        if vm.package_registry {
            warn!(
                "The 'package_registry' field in vm.yaml is deprecated. \
                Please move it to ~/.vm/config.yaml under 'services.package_registry.enabled'"
            );
            global.services.package_registry.enabled = true;
            vm.package_registry = false;
        }

        Ok(())
    }
}
