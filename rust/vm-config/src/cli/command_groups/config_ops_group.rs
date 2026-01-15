use std::path::PathBuf;
use vm_core::error::Result;

use crate::cli::commands;

/// Handle all configuration operation commands
pub struct ConfigOpsGroup;

impl ConfigOpsGroup {
    /// Execute set command
    pub fn execute_set(field: String, values: Vec<String>, global: bool) -> Result<()> {
        commands::config_ops::execute_set(field, values, global)
    }

    /// Execute get command
    pub fn execute_get(field: Option<String>, global: bool) -> Result<()> {
        commands::config_ops::execute_get(field, global)
    }

    /// Execute unset command
    pub fn execute_unset(field: String, global: bool) -> Result<()> {
        commands::config_ops::execute_unset(field, global)
    }

    /// Execute validate command
    pub fn execute_validate(file: Option<PathBuf>, verbose: bool) {
        commands::validation::execute_validate(file, verbose)
    }

    /// Execute dump command
    pub fn execute_dump(file: Option<PathBuf>) -> Result<()> {
        commands::dump::execute_dump(file)
    }

    /// Execute export command
    pub fn execute_export(file: Option<PathBuf>) -> Result<()> {
        commands::dump::execute_export(file)
    }
}
