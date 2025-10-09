//! Configuration operations for VM Tool.
//!
//! This module provides high-level operations for managing VM configuration files,
//! including setting, getting, and unsetting configuration values using dot notation,
//! applying presets, and managing configuration state with dry-run capabilities.
//! It serves as the core configuration manipulation engine for the CLI.

// Internal module declarations
mod get;
mod io;
mod preset;
mod set;
mod unset;

// Public modules for specific functionalities
pub mod port_placeholders;

// Re-export the public API functions for direct use
pub use io::load_global_config;

// Internal imports
use vm_core::error::Result;

/// Configuration operations for VM configuration management.
///
/// Provides high-level operations for reading, writing, and manipulating
/// VM configuration files. Supports both local project configurations
/// and global user settings.
pub struct ConfigOps;

impl ConfigOps {
    /// Set a configuration value using dot notation.
    pub fn set(field: &str, value: &str, global: bool, dry_run: bool) -> Result<()> {
        set::set(field, value, global, dry_run)
    }

    /// Get a configuration value or display entire configuration.
    pub fn get(field: Option<&str>, global: bool) -> Result<()> {
        get::get(field, global)
    }

    /// Unset (remove) a configuration field.
    pub fn unset(field: &str, global: bool) -> Result<()> {
        unset::unset(field, global)
    }

    /// Clear (delete) configuration file.
    pub fn clear(global: bool) -> Result<()> {
        unset::clear(global)
    }

    /// Apply preset(s) to configuration.
    pub fn preset(preset_names: &str, global: bool, list: bool, show: Option<&str>) -> Result<()> {
        preset::preset(preset_names, global, list, show)
    }
}
