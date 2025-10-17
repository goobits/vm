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
pub mod config_ops;
pub mod detector;
mod embedded_presets;
pub mod global_config;
pub mod limit_parser; // Shared limit parsing logic
pub mod loader;
pub mod merge;
mod paths; // Internal only
pub mod ports;
mod preset; // Internal only
pub mod resources; // VM resource suggestions
pub mod schema; // Schema-aware type detection
pub mod validate;
pub mod validator;
pub mod yaml; // YAML operations module

#[cfg(test)]
mod test_memory;

#[cfg(test)]
mod global_config_tests;

// Re-export commonly needed path utilities
pub use paths::{get_current_gid, get_current_uid, get_tool_dir, resolve_tool_path};

// Re-export config operations for use by main vm binary
pub use config_ops::{load_global_config, ConfigOps};

// Re-export global config for use by other crates
pub use global_config::GlobalConfig;

// Re-export CLI utilities
pub use cli::init_config_file;
pub use detector::detect_worktrees;

use std::path::PathBuf;
use vm_core::error::Result;
use vm_core::error::VmError;

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
        let global = GlobalConfig::load()
            .map_err(|e| VmError::Config(format!("Failed to load global configuration: {e}")))?;

        // Load VM configuration with all merging logic
        let mut vm = config::VmConfig::load(config_path.clone())?;

        // Handle host integrations
        if vm.copy_git_config {
            vm.git_config = Some(detector::git::detect_git_config()?);
        }

        if vm.vm.as_ref().and_then(|v| v.timezone.as_deref()) == Some("auto") {
            let detected_timezone = detector::os::detect_timezone();
            if let Some(vm_settings) = vm.vm.as_mut() {
                vm_settings.timezone = Some(detected_timezone);
            }
        }

        // Handle backward compatibility: if deprecated fields are in vm.yaml,
        // warn and apply them to the runtime global config (but don't save)
        let mut runtime_global = global.clone();
        Self::handle_deprecated_fields_raw(config_path, &mut runtime_global)?;

        Ok(Self {
            global: runtime_global,
            vm,
        })
    }

    /// Handle deprecated fields for backward compatibility (deprecated fields removed in v2.0.0+)
    pub fn handle_deprecated_fields_raw(
        _vm_yaml_path: Option<PathBuf>,
        _global: &mut GlobalConfig,
    ) -> Result<()> {
        // All deprecated vm.yaml service flags have been removed in v2.0.0+
        // Services are now configured purely through global config (~/.vm/config.yaml)
        Ok(())
    }
}
