use anyhow::{Result, Context};
use serde_yaml::{Value, Mapping};
use std::path::PathBuf;
use std::fs;
use crate::cli::OutputFormat;

/// YAML operations for yq replacement functionality
pub struct YamlOperations;

impl YamlOperations {
    /// Validate that a file is valid YAML
    pub fn validate_file(file: &PathBuf) -> Result<()> {
        let content = fs::read_to_string(file)
            .with_context(|| format!("Failed to read file: {:?}", file))?;

        let _: Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid YAML in file: {:?}", file))?;

        Ok(())
    }

    /// Add an item to a YAML array at the specified path
    pub fn array_add(file: &PathBuf, path: &str, item: &str) -> Result<()> {
        let content = fs::read_to_string(file)
            .with_context(|| format!("Failed to read file: {:?}", file))?;

        let mut value: Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid YAML in file: {:?}", file))?;

        // Parse the item as YAML
        let new_item: Value = serde_yaml::from_str(item)
            .with_context(|| format!("Invalid YAML item: {}", item))?;

        // Navigate to the path and add the item
        let path_parts: Vec<&str> = path.split('.').collect();
        Self::add_to_array_at_path(&mut value, &path_parts, new_item)?;

        // Write back to file
        let updated_yaml = serde_yaml::to_string(&value)
            .with_context(|| "Failed to serialize YAML")?;

        fs::write(file, updated_yaml)
            .with_context(|| format!("Failed to write file: {:?}", file))?;

        Ok(())
    }

    /// Remove items from a YAML array based on filter
    pub fn array_remove(file: &PathBuf, path: &str, filter: &str) -> Result<()> {
        let content = fs::read_to_string(file)
            .with_context(|| format!("Failed to read file: {:?}", file))?;

        let mut value: Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid YAML in file: {:?}", file))?;

        // Navigate to the path and remove matching items
        let path_parts: Vec<&str> = path.split('.').collect();
        Self::remove_from_array_at_path(&mut value, &path_parts, filter)?;

        // Write back to file
        let updated_yaml = serde_yaml::to_string(&value)
            .with_context(|| "Failed to serialize YAML")?;

        fs::write(file, updated_yaml)
            .with_context(|| format!("Failed to write file: {:?}", file))?;

        Ok(())
    }

    /// Query with conditional filtering
    pub fn filter(file: &PathBuf, expression: &str, output_format: &OutputFormat) -> Result<()> {
        let content = fs::read_to_string(file)
            .with_context(|| format!("Failed to read file: {:?}", file))?;

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

    // Helper function to navigate to array and add item
    fn add_to_array_at_path(value: &mut Value, path: &[&str], item: Value) -> Result<()> {
        if path.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }

        if path.len() == 1 {
            // We're at the target array
            match value {
                Value::Sequence(seq) => {
                    seq.push(item);
                    return Ok(());
                }
                Value::Mapping(map) => {
                    let key = Value::String(path[0].to_string());
                    match map.get_mut(&key) {
                        Some(Value::Sequence(seq)) => {
                            seq.push(item);
                            return Ok(());
                        }
                        Some(_) => return Err(anyhow::anyhow!("Path '{}' is not an array", path[0])),
                        None => {
                            // Create new array
                            map.insert(key, Value::Sequence(vec![item]));
                            return Ok(());
                        }
                    }
                }
                _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
            }
        }

        // Navigate deeper
        match value {
            Value::Mapping(map) => {
                let key = Value::String(path[0].to_string());
                match map.get_mut(&key) {
                    Some(nested) => Self::add_to_array_at_path(nested, &path[1..], item)?,
                    None => {
                        // Create nested structure
                        let mut nested = Value::Mapping(Mapping::new());
                        Self::add_to_array_at_path(&mut nested, &path[1..], item)?;
                        map.insert(key, nested);
                    }
                }
            }
            _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
        }

        Ok(())
    }

    // Helper function to navigate to array and remove items
    fn remove_from_array_at_path(value: &mut Value, path: &[&str], filter: &str) -> Result<()> {
        if path.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }

        if path.len() == 1 {
            // We're at the target array
            match value {
                Value::Mapping(map) => {
                    let key = Value::String(path[0].to_string());
                    match map.get_mut(&key) {
                        Some(Value::Sequence(seq)) => {
                            seq.retain(|item| !Self::matches_filter(item, filter));
                            return Ok(());
                        }
                        Some(_) => return Err(anyhow::anyhow!("Path '{}' is not an array", path[0])),
                        None => return Err(anyhow::anyhow!("Path '{}' not found", path[0])),
                    }
                }
                _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
            }
        }

        // Navigate deeper
        match value {
            Value::Mapping(map) => {
                let key = Value::String(path[0].to_string());
                match map.get_mut(&key) {
                    Some(nested) => Self::remove_from_array_at_path(nested, &path[1..], filter)?,
                    None => return Err(anyhow::anyhow!("Path '{}' not found", path[0])),
                }
            }
            _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
        }

        Ok(())
    }

    // Simple filter matching (can be enhanced with more complex expressions)
    fn matches_filter(value: &Value, filter: &str) -> bool {
        // Support basic equality filters like: .source == "value"
        if filter.starts_with(".") {
            if let Some(eq_pos) = filter.find(" == ") {
                let field = &filter[1..eq_pos].trim();
                let expected = filter[eq_pos + 4..].trim().trim_matches('"');

                return Self::get_field_value(value, field)
                    .map(|v| v.as_str().unwrap_or("") == expected)
                    .unwrap_or(false);
            }
        }

        // Fallback: simple string matching
        let value_str = match value {
            Value::String(s) => s.as_str(),
            _ => return false,
        };

        value_str.contains(filter)
    }

    // Get field value from YAML value
    fn get_field_value<'a>(value: &'a Value, field: &str) -> Option<&'a Value> {
        match value {
            Value::Mapping(map) => map.get(&Value::String(field.to_string())),
            _ => None,
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
                        if let Some(Value::Sequence(seq)) = map.get(&Value::String("mounts".to_string())) {
                            let results: Vec<Value> = seq.iter()
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