//! YAML query operations for field extraction and filtering.
//!
//! This module provides functionality for querying YAML structures using dot notation
//! and filtering sequences based on field expressions. It supports extracting nested
//! values and applying filters to arrays of data.

use super::core::CoreOperations;
use crate::cli::OutputFormat;
use serde_yaml::Value;
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;
use vm_core::error::{Result, VmError};

/// Query-specific YAML operations
pub struct QueryOperations;

impl QueryOperations {
    /// Query with conditional filtering
    pub fn filter(file: &PathBuf, expression: &str, output_format: &OutputFormat) -> Result<()> {
        let content = CoreOperations::read_file_or_stdin(file)?;

        let value: Value = serde_yaml::from_str(&content).map_err(|e| {
            VmError::Serialization(format!("Invalid YAML in file: {:?}: {}", file, e))
        })?;

        // Apply the filter expression
        let result = Self::apply_filter(&value, expression);

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
                    let Value::Mapping(map) = item else {
                        continue;
                    };

                    let field_key = Value::String(field.to_string());
                    let Some(field_value) = map.get(&field_key) else {
                        continue;
                    };

                    let Some(field_str) = field_value.as_str() else {
                        continue;
                    };

                    if field_str == match_value {
                        matching_items.push(item.clone());
                    }
                }
                Value::Sequence(matching_items)
            }
            _ => {
                return Err(VmError::Config(
                    "Path does not point to an array".to_string(),
                ))
            }
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
    fn apply_filter(value: &Value, expression: &str) -> Value {
        // Handle array access with filters like: .mounts[] | select(.source == "value")
        if !expression.contains("[]") {
            return value.clone();
        }

        let Some(array_part) = expression.split("[]").next() else {
            return value.clone();
        };

        let array_path = array_part.trim_start_matches('.');

        // Handle any array field, not just "mounts"
        Self::filter_array_field(value, array_path, expression)
    }

    // Extract array filtering logic for any field
    fn filter_array_field(value: &Value, field_name: &str, expression: &str) -> Value {
        let Value::Mapping(map) = value else {
            return value.clone();
        };

        let Some(Value::Sequence(seq)) = map.get(Value::String(field_name.to_string())) else {
            return value.clone();
        };

        // Extract the filter condition from the expression
        // e.g., ".mounts[] | select(.source == \"value\")" -> "select(.source == \"value\")"
        let filter_part = if let Some(pipe_pos) = expression.find(" | ") {
            &expression[pipe_pos + 3..] // Skip " | "
        } else {
            // If no pipe, just use the expression as-is
            expression
        };

        let results: Vec<Value> = seq
            .iter()
            .filter(|item| {
                // Implement actual filter logic based on expression
                Self::evaluate_filter_expression(item, filter_part)
            })
            .cloned()
            .collect();

        Value::Sequence(results)
    }

    /// Evaluate a filter expression against a YAML value
    /// Supports patterns like:
    /// - `.field` - Check if field exists and is not null
    /// - `select(.field == "value")` - Check if field equals specific value
    /// - `select(.field == value)` - Check if field equals unquoted value
    fn evaluate_filter_expression(item: &Value, expression: &str) -> bool {
        let expression = expression.trim();

        // Handle select() expressions like: select(.source == "value")
        if expression.starts_with("select(") && expression.ends_with(')') {
            let inner = &expression[7..expression.len() - 1]; // Remove "select(" and ")"
            return Self::evaluate_select_condition(item, inner);
        }

        // Handle simple field existence checks like: .source
        if expression.starts_with('.') {
            let field_name = expression.strip_prefix('.').unwrap_or(expression); // Remove the leading dot if present

            return match item {
                Value::Mapping(map) => {
                    // Check if the field exists and has a truthy value
                    map.get(Value::String(field_name.to_string()))
                        .map(|value| !matches!(value, Value::Null))
                        .unwrap_or(false)
                }
                _ => false,
            };
        }

        // For non-recognized expressions, default to true to maintain existing behavior
        true
    }

    /// Evaluate a condition inside select() like: .field == "value"
    fn evaluate_select_condition(item: &Value, condition: &str) -> bool {
        let condition = condition.trim();

        // Handle equality comparisons: .field == "value" or .field == value
        if let Some(eq_pos) = condition.find("==") {
            let field_part = condition[..eq_pos].trim();
            let value_part = condition[eq_pos + 2..].trim();

            // Extract field name (remove leading dot)
            if !field_part.starts_with('.') {
                return false;
            }
            let field_name = &field_part[1..];

            // Get the field value from the item
            let Value::Mapping(map) = item else {
                return false;
            };

            let Some(field_value) = map.get(Value::String(field_name.to_string())) else {
                return false;
            };

            // Parse the expected value (handle quoted and unquoted strings)
            let expected_value = if value_part.starts_with('"') && value_part.ends_with('"') {
                // Quoted string - remove quotes
                &value_part[1..value_part.len() - 1]
            } else {
                // Unquoted value
                value_part
            };

            // Compare values
            match field_value {
                Value::String(s) => s == expected_value,
                Value::Number(n) => {
                    // Try to parse expected_value as a number
                    if let Ok(expected) = expected_value.parse::<i64>() {
                        n.as_i64() == Some(expected)
                    } else if let (Ok(expected), Some(actual)) =
                        (expected_value.parse::<f64>(), n.as_f64())
                    {
                        (actual - expected).abs() < f64::EPSILON
                    } else {
                        false
                    }
                }
                Value::Bool(b) => expected_value
                    .parse::<bool>()
                    .ok()
                    .map(|expected| *b == expected)
                    .unwrap_or(false),
                _ => false,
            }
        } else {
            // For non-equality conditions, just check field existence
            if condition.starts_with('.') {
                let field_name = condition.strip_prefix('.').unwrap_or(condition); // Remove the leading dot if present
                match item {
                    Value::Mapping(map) => map
                        .get(Value::String(field_name.to_string()))
                        .map(|value| !matches!(value, Value::Null))
                        .unwrap_or(false),
                    _ => false,
                }
            } else {
                false
            }
        }
    }
}
