use anyhow::Result;
use std::path::PathBuf;

use crate::cli::commands;

/// Handle all configuration operation commands
pub struct ConfigOpsGroup;

impl ConfigOpsGroup {
    /// Execute set command
    pub fn execute_set(field: String, value: String, global: bool) -> Result<()> {
        commands::config_ops::execute_set(field, value, global)
    }

    /// Execute get command
    pub fn execute_get(field: Option<String>, global: bool) -> Result<()> {
        commands::config_ops::execute_get(field, global)
    }

    /// Execute unset command
    pub fn execute_unset(field: String, global: bool) -> Result<()> {
        commands::config_ops::execute_unset(field, global)
    }

    /// Execute clear command
    pub fn execute_clear(global: bool) -> Result<()> {
        commands::config_ops::execute_clear(global)
    }

    /// Execute validate command
    pub fn execute_validate(file: Option<PathBuf>, no_preset: bool, verbose: bool) {
        commands::validation::execute_validate(file, no_preset, verbose)
    }

    /// Execute dump command
    pub fn execute_dump(file: Option<PathBuf>, no_preset: bool) -> Result<()> {
        commands::dump::execute_dump(file, no_preset)
    }

    /// Execute export command
    pub fn execute_export(file: Option<PathBuf>, no_preset: bool) -> Result<()> {
        commands::dump::execute_export(file, no_preset)
    }
}
