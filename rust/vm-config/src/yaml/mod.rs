// YAML operations organized by functionality

pub mod array_ops;
pub mod core;
pub mod field_ops;
pub mod formatter;
pub mod query_ops;
pub mod transform_ops;

// Re-export the main operations struct for backwards compatibility
pub use array_ops::ArrayOperations;
pub use core::CoreOperations;
pub use field_ops::FieldOperations;
pub use query_ops::QueryOperations;
pub use transform_ops::TransformOperations;

use crate::cli::{OutputFormat, TransformFormat};
use anyhow::Result;
use std::path::PathBuf;

/// Main YAML operations interface - delegates to specialized modules
pub struct YamlOperations;

impl YamlOperations {
    // Array operations
    pub fn array_add(file: &PathBuf, path: &str, item: &str) -> Result<()> {
        ArrayOperations::add(file, path, item)
    }

    pub fn array_remove(file: &PathBuf, path: &str, filter: &str) -> Result<()> {
        ArrayOperations::remove(file, path, filter)
    }

    pub fn array_length(file: &PathBuf, path: &str) -> Result<usize> {
        ArrayOperations::length(file, path)
    }

    pub fn add_to_array_path(file: &PathBuf, path: &str, object: &str, stdout: bool) -> Result<()> {
        ArrayOperations::add_object_to_path(file, path, object, stdout)
    }

    // Field operations
    pub fn modify_file(file: &PathBuf, field: &str, value: &str, stdout: bool) -> Result<()> {
        FieldOperations::modify(file, field, value, stdout)
    }

    pub fn has_field(file: &PathBuf, field: &str, subfield: &str) -> Result<bool> {
        FieldOperations::has_field(file, field, subfield)
    }

    // Query operations
    pub fn filter(file: &PathBuf, expression: &str, output_format: &OutputFormat) -> Result<()> {
        QueryOperations::filter(file, expression, output_format)
    }

    pub fn select_where(
        file: &PathBuf,
        path: &str,
        field: &str,
        value: &str,
        format: &OutputFormat,
    ) -> Result<()> {
        QueryOperations::select_where(file, path, field, value, format)
    }

    pub fn count_items(file: &PathBuf, path: &str) -> Result<usize> {
        QueryOperations::count_items(file, path)
    }

    // Transform operations
    pub fn transform(file: &PathBuf, expression: &str, format: &TransformFormat) -> Result<()> {
        TransformOperations::transform(file, expression, format)
    }

    pub fn merge_eval_all(files: &[PathBuf], format: &OutputFormat) -> Result<()> {
        TransformOperations::merge_eval_all(files, format)
    }

    // Core operations
    pub fn validate_file(file: &PathBuf) -> Result<()> {
        CoreOperations::validate_file(file)
    }

    pub fn delete_from_array(
        file: &PathBuf,
        path: &str,
        field: &str,
        value: &str,
        format: &OutputFormat,
    ) -> Result<()> {
        ArrayOperations::delete_matching(file, path, field, value, format)
    }
}
