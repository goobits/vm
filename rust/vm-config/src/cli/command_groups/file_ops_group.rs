use anyhow::Result;
use std::path::PathBuf;

use crate::cli::commands;
use crate::cli::{OutputFormat, TransformFormat};

/// Handle all file operation commands
pub struct FileOpsGroup;

impl FileOpsGroup {
    /// Execute merge command
    pub fn execute_merge(base: PathBuf, overlay: Vec<PathBuf>, format: OutputFormat) -> Result<()> {
        commands::merge::execute_merge(base, overlay, format)
    }

    /// Execute convert command
    pub fn execute_convert(input: PathBuf, format: OutputFormat) -> Result<()> {
        commands::conversion::execute(input, format)
    }

    /// Execute array add command
    pub fn execute_array_add(file: PathBuf, path: String, item: String) -> Result<()> {
        commands::file_ops::execute_array_add(file, path, item)
    }

    /// Execute array remove command
    pub fn execute_array_remove(file: PathBuf, path: String, filter: String) -> Result<()> {
        commands::file_ops::execute_array_remove(file, path, filter)
    }

    /// Execute modify command
    pub fn execute_modify(file: PathBuf, field: String, value: String, stdout: bool) -> Result<()> {
        commands::file_ops::execute_modify(file, field, value, stdout)
    }

    /// Execute add to array command
    pub fn execute_add_to_array(
        file: PathBuf,
        path: String,
        object: String,
        stdout: bool,
    ) -> Result<()> {
        commands::file_ops::execute_add_to_array(file, path, object, stdout)
    }

    /// Execute delete command
    pub fn execute_delete(
        file: PathBuf,
        path: String,
        field: String,
        value: String,
        format: OutputFormat,
    ) -> Result<()> {
        commands::file_ops::execute_delete(file, path, field, value, format)
    }

    /// Execute check file command
    pub fn execute_check_file(file: PathBuf) -> Result<()> {
        commands::validation::execute_check_file(file)
    }

    /// Execute merge eval all command
    pub fn execute_merge_eval_all(files: Vec<PathBuf>, format: OutputFormat) -> Result<()> {
        commands::merge::execute_merge_eval_all(files, format)
    }

    /// Execute transform command
    pub fn execute_transform(
        file: PathBuf,
        expression: String,
        format: TransformFormat,
    ) -> Result<()> {
        commands::transformation::execute(file, expression, format)
    }
}
