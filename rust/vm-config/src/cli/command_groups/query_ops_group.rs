use std::path::PathBuf;
use vm_core::error::Result;

use crate::cli::commands;
use crate::cli::OutputFormat;

/// Handle all query operation commands
pub struct QueryOpsGroup;

impl QueryOpsGroup {
    /// Execute query command
    pub fn execute_query(
        config: PathBuf,
        field: String,
        raw: bool,
        default: Option<String>,
    ) -> Result<()> {
        commands::query::execute_query(config, field, raw, default)
    }

    /// Execute filter command
    pub fn execute_filter(
        file: PathBuf,
        expression: String,
        output_format: OutputFormat,
    ) -> Result<()> {
        commands::query::execute_filter(file, expression, output_format)
    }

    /// Execute array length command
    pub fn execute_array_length(file: PathBuf, path: String) -> Result<()> {
        commands::query::execute_array_length(file, path)
    }

    /// Execute has field command
    pub fn execute_has_field(file: PathBuf, field: String, subfield: String) -> Result<()> {
        commands::query::execute_has_field(file, field, subfield)
    }

    /// Execute select where command
    pub fn execute_select_where(
        file: PathBuf,
        path: String,
        field: String,
        value: String,
        format: OutputFormat,
    ) -> Result<()> {
        commands::query::execute_select_where(file, path, field, value, format)
    }

    /// Execute count command
    pub fn execute_count(file: PathBuf, path: String) -> Result<()> {
        commands::query::execute_count(file, path)
    }
}
