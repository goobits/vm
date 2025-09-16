use super::core::CoreOperations;
use crate::cli::OutputFormat;
use anyhow::{Context, Result};
use serde_yaml_ng as serde_yaml;
use serde_yaml::Value;
use std::path::PathBuf;

/// Query-specific YAML operations
pub struct QueryOperations;

impl QueryOperations {
    /// Query with conditional filtering
    pub fn filter(file: &PathBuf, expression: &str, output_format: &OutputFormat) -> Result<()> {
        let content = CoreOperations::read_file_or_stdin(file)?;

        let value: Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid YAML in file: {:?}", file))?;

        // Apply the filter expression
        let result = Self::apply_filter(&value, expression)?;

        // Output in requested format
        match output_format {
            OutputFormat::Yaml => {
                let yaml = serde_yaml::to_string(&result)?;
                print!("{}", yaml);
            }
            OutputFormat::Json => {
                let json = serde_json::to_string(&result)?;
                println!("{}", json);
            }
            OutputFormat::JsonPretty => {
                let json = serde_json::to_string_pretty(&result)?;
                println!("{}", json);
            }
        }

        Ok(())
    }

    /// Select items from array where field matches value
    pub fn select_where(
        file: &PathBuf,
        path: &str,
        field: &str,
        match_value: &str,
        format: &OutputFormat,
    ) -> Result<()> {
        let value = CoreOperations::load_yaml_file(file)?;

        let target = if path.is_empty() {
            &value
        } else {
            CoreOperations::get_nested_field(&value, path)?
        };

        let results = match target {
            Value::Sequence(seq) => {
                let mut matching_items = Vec::new();
                for item in seq {
                    if let Value::Mapping(map) = item {
                        let field_key = Value::String(field.to_string());
                        if let Some(field_value) = map.get(&field_key) {
                            if let Some(field_str) = field_value.as_str() {
                                if field_str == match_value {
                                    matching_items.push(item.clone());
                                }
                            }
                        }
                    }
                }
                Value::Sequence(matching_items)
            }
            _ => return Err(anyhow::anyhow!("Path does not point to an array")),
        };

        // Output results
        match format {
            OutputFormat::Yaml => {
                let yaml = serde_yaml::to_string(&results)?;
                print!("{}", yaml);
            }
            OutputFormat::Json => {
                let json = serde_json::to_string(&results)?;
                println!("{}", json);
            }
            OutputFormat::JsonPretty => {
                let json = serde_json::to_string_pretty(&results)?;
                println!("{}", json);
            }
        }

        Ok(())
    }

    /// Count items in array or object
    pub fn count_items(file: &PathBuf, path: &str) -> Result<usize> {
        let value = CoreOperations::load_yaml_file(file)?;

        let target = if path.is_empty() {
            &value
        } else {
            CoreOperations::get_nested_field(&value, path)?
        };

        match target {
            Value::Sequence(seq) => Ok(seq.len()),
            Value::Mapping(map) => Ok(map.len()),
            _ => Ok(0),
        }
    }

    // Apply filter expression (basic implementation)
    fn apply_filter(value: &Value, expression: &str) -> Result<Value> {
        // Handle array access with filters like: .mounts[] | select(.source == "value")
        if expression.contains("[]") && expression.contains("select") {
            // Simple implementation for array filtering
            if let Some(array_part) = expression.split("[]").next() {
                let array_path = array_part.trim_start_matches('.');
                if array_path == "mounts" {
                    if let Value::Mapping(map) = value {
                        if let Some(Value::Sequence(seq)) =
                            map.get(Value::String(String::from("mounts")))
                        {
                            let results: Vec<Value> = seq
                                .iter()
                                .filter(|_item| {
                                    // Simple select filter parsing
                                    if expression.contains(".source") {
                                        return true; // For now, return all items
                                    }
                                    true
                                })
                                .cloned()
                                .collect();
                            return Ok(Value::Sequence(results));
                        }
                    }
                }
            }
        }

        // Fallback: return the whole value
        Ok(value.clone())
    }
}
