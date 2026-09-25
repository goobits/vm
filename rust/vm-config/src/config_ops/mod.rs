//! Configuration operations for VM Tool.
//!
//! This module provides high-level operations for managing VM configuration files,
//! including setting, getting, and unsetting configuration values using dot notation,
//! applying presets, and managing configuration state with dry-run capabilities.
//! It serves as the core configuration manipulation engine for the CLI.

// Internal module declarations
mod get;
mod init;
mod io;
pub(crate) mod preset;
mod set;
mod unset;
mod validate;

// Public modules for specific functionalities
pub mod port_placeholders;

// Re-export the public API functions for direct use
pub use init::init_config_file;

// Internal imports
use std::path::PathBuf;
use vm_core::error::Result;

/// Configuration operations for VM configuration management.
///
/// Provides high-level operations for reading, writing, and manipulating
/// VM configuration files. Supports both local project configurations
/// and global user settings.
pub struct ConfigOps;

impl ConfigOps {
    /// Set a configuration value using dot notation with schema-aware type detection.
    /// Accepts multiple values for array fields.
    pub fn set(field: &str, values: &[String], global: bool, dry_run: bool) -> Result<()> {
        Self::set_at(field, values, global, dry_run, None)
    }

    pub fn set_at(
        field: &str,
        values: &[String],
        global: bool,
        dry_run: bool,
        path: Option<PathBuf>,
    ) -> Result<()> {
        set::set(field, values, global, dry_run, path, None)
    }

    pub fn set_json_at(field: &str, json: &str, global: bool, path: Option<PathBuf>) -> Result<()> {
        set::set(field, &[], global, false, path, Some(json))
    }

    /// Get a configuration value or display entire configuration.
    pub fn get(field: Option<&str>, global: bool) -> Result<()> {
        get::get(field, global)
    }

    /// Unset (remove) a configuration field.
    pub fn unset(field: &str, global: bool) -> Result<()> {
        Self::unset_at(field, global, None)
    }

    pub fn unset_at(field: &str, global: bool, path: Option<PathBuf>) -> Result<()> {
        unset::unset(field, global, path)
    }

    /// Clear (delete) configuration file.
    pub fn clear(global: bool) -> Result<()> {
        unset::clear(global)
    }

    /// Apply preset(s) to configuration.
    pub fn preset(preset_names: &str, global: bool, list: bool, show: Option<&str>) -> Result<()> {
        Self::preset_at(preset_names, global, list, show, None)
    }

    pub fn preset_at(
        preset_names: &str,
        global: bool,
        list: bool,
        show: Option<&str>,
        path: Option<PathBuf>,
    ) -> Result<()> {
        preset::preset(preset_names, global, list, show, path)
    }
}
