use super::core::CoreOperations;
use crate::cli::OutputFormat;
use serde_yaml::{Mapping, Value};
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;
use vm_core::error::{Result, VmError};

/// Array-specific YAML operations
pub struct ArrayOperations;

impl ArrayOperations {
    /// Add an item to a YAML array at the specified path
    pub fn add(file: &PathBuf, path: &str, item: &str) -> Result<()> {
        let content = CoreOperations::read_file_or_stdin(file)?;

        let mut value: Value = serde_yaml::from_str(&content)
            .map_err(|e| VmError::Serialization(format!("Invalid YAML in file: {file:?}: {e}")))?;

        // Parse the item as YAML
        let new_item: Value = serde_yaml::from_str(item)
            .map_err(|e| VmError::Serialization(format!("Invalid YAML item: {item}: {e}")))?;

        // Navigate to the path and add the item
        let path_parts: Vec<&str> = path.split('.').collect();
        Self::add_to_array_at_path(&mut value, &path_parts, new_item)?;

        CoreOperations::write_yaml_file(file, &value)?;
        Ok(())
    }

    /// Remove items from a YAML array based on filter
    pub fn remove(file: &PathBuf, path: &str, filter: &str) -> Result<()> {
        let content = CoreOperations::read_file_or_stdin(file)?;

        let mut value: Value = serde_yaml::from_str(&content)
            .map_err(|e| VmError::Serialization(format!("Invalid YAML in file: {file:?}: {e}")))?;

        // Navigate to the path and remove matching items
        let path_parts: Vec<&str> = path.split('.').collect();
        Self::remove_from_array_at_path(&mut value, &path_parts, filter)?;

        CoreOperations::write_yaml_file(file, &value)?;
        Ok(())
    }

    /// Get array length
    pub fn length(file: &PathBuf, path: &str) -> Result<usize> {
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

    /// Add object to array at specified path
    pub fn add_object_to_path(
        file: &PathBuf,
        path: &str,
        object_json: &str,
        stdout: bool,
    ) -> Result<()> {
        let mut value = CoreOperations::load_yaml_file(file)?;

        // Parse the JSON object and convert to YAML
        let json_value: serde_json::Value = serde_json::from_str(object_json).map_err(|e| {
            VmError::Serialization(format!("Failed to parse JSON object: {object_json}: {e}"))
        })?;

        // Convert via string to avoid type compatibility issues
        let yaml_string = serde_yaml::to_string(&json_value)?;
        let yaml_value: Value = serde_yaml::from_str(&yaml_string)
            .map_err(|e| VmError::Serialization(format!("Failed to convert JSON to YAML: {e}")))?;

        // Navigate to the path and add the object
        let path_parts: Vec<&str> = path.split('.').collect();
        Self::add_object_to_array_at_path(&mut value, &path_parts, yaml_value)?;

        if stdout {
            let yaml = serde_yaml::to_string(&value)?;
            print!("{yaml}");
        } else {
            CoreOperations::write_yaml_file(file, &value)?;
        }

        Ok(())
    }

    /// Delete items from array matching a condition
    pub fn delete_matching(
        file: &PathBuf,
        path: &str,
        field: &str,
        match_value: &str,
        format: &OutputFormat,
    ) -> Result<()> {
        let mut value = CoreOperations::load_yaml_file(file)?;

        let target = if path.is_empty() {
            &mut value
        } else {
            Self::get_nested_field_mut(&mut value, path)?
        };

        match target {
            Value::Sequence(seq) => {
                seq.retain(|item| {
                    let Value::Mapping(map) = item else {
                        return true;
                    };

                    let field_key = Value::String(field.to_string());
                    let Some(field_value) = map.get(&field_key) else {
                        return true;
                    };

                    let Some(field_str) = field_value.as_str() else {
                        return true;
                    };

                    field_str != match_value
                });
            }
            _ => {
                return Err(VmError::Config(
                    "Path does not point to an array".to_string(),
                ))
            }
        }

        // Output results
        match format {
            OutputFormat::Yaml => {
                let yaml = serde_yaml::to_string(&value)?;
                print!("{yaml}");
            }
            OutputFormat::Json => {
                let json = serde_json::to_string(&value)?;
                println!("{json}");
            }
            OutputFormat::JsonPretty => {
                let json = serde_json::to_string_pretty(&value)?;
                println!("{json}");
            }
        }

        Ok(())
    }

    // Helper function to navigate to array and add item
    fn add_to_array_at_path(value: &mut Value, path: &[&str], item: Value) -> Result<()> {
        if path.is_empty() {
            return Err(VmError::Config("Empty path".to_string()));
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
                        Some(_) => {
                            return Err(VmError::Config(format!(
                                "Path '{}' is not an array",
                                path[0]
                            )))
                        }
                        None => {
                            // Create new array
                            map.insert(key, Value::Sequence(vec![item]));
                            return Ok(());
                        }
                    }
                }
                _ => {
                    return Err(VmError::Config(
                        "Cannot navigate path on non-object".to_string(),
                    ))
                }
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
            _ => {
                return Err(VmError::Config(
                    "Cannot navigate path on non-object".to_string(),
                ))
            }
        }

        Ok(())
    }

    // Helper function to navigate to array and remove items
    fn remove_from_array_at_path(value: &mut Value, path: &[&str], filter: &str) -> Result<()> {
        if path.is_empty() {
            return Err(VmError::Config("Empty path".to_string()));
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
                        Some(_) => {
                            return Err(VmError::Config(format!(
                                "Path '{}' is not an array",
                                path[0]
                            )))
                        }
                        None => {
                            return Err(VmError::Config(format!("Path '{}' not found", path[0])))
                        }
                    }
                }
                _ => {
                    return Err(VmError::Config(
                        "Cannot navigate path on non-object".to_string(),
                    ))
                }
            }
        }

        // Navigate deeper
        match value {
            Value::Mapping(map) => {
                let key = Value::String(path[0].to_string());
                match map.get_mut(&key) {
                    Some(nested) => Self::remove_from_array_at_path(nested, &path[1..], filter)?,
                    None => return Err(VmError::Config(format!("Path '{}' not found", path[0]))),
                }
            }
            _ => {
                return Err(VmError::Config(
                    "Cannot navigate path on non-object".to_string(),
                ))
            }
        }

        Ok(())
    }

    // Helper function for adding objects to arrays
    fn add_object_to_array_at_path(value: &mut Value, path: &[&str], object: Value) -> Result<()> {
        Self::add_to_array_at_path(value, path, object)
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
            Value::Mapping(map) => map.get(Value::String(field.to_string())),
            _ => None,
        }
    }

    // Get mutable nested field from YAML value using dot notation
    fn get_nested_field_mut<'a>(value: &'a mut Value, path: &str) -> Result<&'a mut Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            match current {
                Value::Mapping(map) => {
                    let key = Value::String(part.to_string());
                    current = map
                        .get_mut(&key)
                        .ok_or_else(|| VmError::Config(format!("Field '{part}' not found")))?;
                }
                _ => {
                    return Err(VmError::Config(format!(
                        "Cannot access field '{part}' on non-object"
                    )))
                }
            }
        }

        Ok(current)
    }
}
