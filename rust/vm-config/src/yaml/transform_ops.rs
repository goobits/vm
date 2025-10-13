use super::core::CoreOperations;
use crate::cli::{OutputFormat, TransformFormat};
use serde_yaml::Value;
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;
use vm_core::error::{Result, VmError};

/// Transform-specific YAML operations
pub struct TransformOperations;

impl TransformOperations {
    /// Transform data with expressions
    pub fn transform(file: &PathBuf, expression: &str, format: &TransformFormat) -> Result<()> {
        let value = CoreOperations::load_yaml_file(file)?;

        let results = if expression.contains("to_entries[]") {
            // Handle to_entries transformations
            Self::transform_to_entries(&value, expression)
        } else if expression.contains(".[]") {
            // Handle array iteration
            Self::transform_array_items(&value, expression)?
        } else {
            vec![expression.to_string()]
        };

        // Output in requested format
        match format {
            TransformFormat::Lines => {
                for result in results {
                    println!("{result}");
                }
            }
            TransformFormat::Space => {
                println!("{}", results.join(" "));
            }
            TransformFormat::Comma => {
                println!("{}", results.join(","));
            }
            TransformFormat::Json => {
                let json = serde_json::to_string(&results)?;
                println!("{json}");
            }
            TransformFormat::Yaml => {
                let yaml = serde_yaml::to_string(&results)?;
                print!("{yaml}");
            }
        }

        Ok(())
    }

    /// Merge multiple YAML files with deep merging
    pub fn merge_eval_all(files: &[PathBuf], format: &OutputFormat) -> Result<()> {
        if files.len() < 2 {
            return Err(VmError::Config(
                "Need at least 2 files to merge".to_string(),
            ));
        }

        // Load first file as base
        let mut result = CoreOperations::load_yaml_file(&files[0])?;

        // Merge subsequent files
        for file in &files[1..] {
            let overlay = CoreOperations::load_yaml_file(file)?;
            result = Self::deep_merge_values(result, overlay);
        }

        // Output result
        match format {
            OutputFormat::Yaml => {
                let yaml = serde_yaml::to_string(&result)?;
                print!("{yaml}");
            }
            OutputFormat::Json => {
                let json = serde_json::to_string(&result)?;
                println!("{json}");
            }
            OutputFormat::JsonPretty => {
                let json = serde_json::to_string_pretty(&result)?;
                println!("{json}");
            }
        }

        Ok(())
    }

    // Deep merge two YAML values
    fn deep_merge_values(base: Value, overlay: Value) -> Value {
        match (base, overlay) {
            (Value::Mapping(mut base_map), Value::Mapping(overlay_map)) => {
                for (key, overlay_value) in overlay_map {
                    match base_map.remove(&key) {
                        Some(base_value) => {
                            base_map
                                .insert(key, Self::deep_merge_values(base_value, overlay_value));
                        }
                        None => {
                            base_map.insert(key, overlay_value);
                        }
                    }
                }
                Value::Mapping(base_map)
            }
            (Value::Sequence(mut base_seq), Value::Sequence(overlay_seq)) => {
                base_seq.extend(overlay_seq);
                Value::Sequence(base_seq)
            }
            (_, overlay) => overlay, // Overlay wins for non-mergeable types
        }
    }

    // Transform entries (basic implementation)
    fn transform_to_entries(value: &Value, _expression: &str) -> Vec<String> {
        match value {
            Value::Mapping(map) => {
                let mut results = Vec::new();
                for (key, value) in map {
                    if let Value::String(key_str) = key {
                        let entry = format!("{}={}", key_str, Self::value_to_string(value));
                        results.push(entry);
                    }
                }
                results
            }
            _ => vec!["Not a mapping".to_string()],
        }
    }

    // Transform array items (basic implementation)
    fn transform_array_items(value: &Value, expression: &str) -> Result<Vec<String>> {
        // Look for the array path in the expression
        if let Some(array_start) = expression.find(".[]") {
            let path = &expression[0..array_start];
            let path = path.trim_start_matches('.');

            let target = if path.is_empty() {
                value
            } else {
                CoreOperations::get_nested_field(value, path)?
            };

            match target {
                Value::Sequence(seq) => {
                    let mut results = Vec::new();
                    for item in seq {
                        results.push(Self::value_to_string(item));
                    }
                    Ok(results)
                }
                _ => Ok(vec!["Not an array".to_string()]),
            }
        } else {
            Ok(vec![expression.to_string()])
        }
    }

    // Convert Value to string representation
    fn value_to_string(value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            _ => format!("{value:?}"),
        }
    }
}
