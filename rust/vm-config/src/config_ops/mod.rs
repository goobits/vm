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
mod plan;
pub(crate) mod preset;
mod set;
mod unset;
mod validate;

pub use plan::ConfigMutationReport;

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
    /// Validate the raw project document, including schema-owned key names,
    /// before applying profiles or other derived settings.
    pub fn validate_file(path: &std::path::Path) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        let value = crate::yaml::CoreOperations::parse_yaml_with_diagnostics(
            &content,
            &path.display().to_string(),
        )?;
        validate::candidate(&value, path, false)
    }
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
        set::set(field, values, global, dry_run, path, None, false).map(|_| ())
    }

    pub fn set_report_at(
        field: &str,
        values: &[String],
        global: bool,
        dry_run: bool,
        path: Option<PathBuf>,
    ) -> Result<ConfigMutationReport> {
        set::set(field, values, global, dry_run, path, None, true)
    }

    pub fn set_json_at(field: &str, json: &str, global: bool, path: Option<PathBuf>) -> Result<()> {
        Self::set_json_preview_at(field, json, global, false, path)
    }

    pub fn set_json_preview_at(
        field: &str,
        json: &str,
        global: bool,
        dry_run: bool,
        path: Option<PathBuf>,
    ) -> Result<()> {
        set::set(field, &[], global, dry_run, path, Some(json), false).map(|_| ())
    }

    pub fn set_json_report_at(
        field: &str,
        json: &str,
        global: bool,
        dry_run: bool,
        path: Option<PathBuf>,
    ) -> Result<ConfigMutationReport> {
        set::set(field, &[], global, dry_run, path, Some(json), true)
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
        Self::unset_preview_at(field, global, false, path)
    }

    pub fn unset_preview_at(
        field: &str,
        global: bool,
        dry_run: bool,
        path: Option<PathBuf>,
    ) -> Result<()> {
        unset::unset(field, global, dry_run, path, false).map(|_| ())
    }

    pub fn unset_report_at(
        field: &str,
        global: bool,
        dry_run: bool,
        path: Option<PathBuf>,
    ) -> Result<ConfigMutationReport> {
        unset::unset(field, global, dry_run, path, true)
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
        Self::preset_preview_at(preset_names, global, list, show, false, path)
    }

    pub fn preset_preview_at(
        preset_names: &str,
        global: bool,
        list: bool,
        show: Option<&str>,
        dry_run: bool,
        path: Option<PathBuf>,
    ) -> Result<()> {
        preset::preset(preset_names, global, list, show, dry_run, path, false).map(|_| ())
    }

    pub fn preset_apply_report_at(
        preset_names: &str,
        global: bool,
        dry_run: bool,
        path: Option<PathBuf>,
    ) -> Result<ConfigMutationReport> {
        preset::apply_report(preset_names, global, dry_run, path)
    }
}
