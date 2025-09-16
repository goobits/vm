use super::core::CoreOperations;
use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};
use std::path::PathBuf;

/// Field-specific YAML operations
pub struct FieldOperations;

impl FieldOperations {
    /// Modify YAML file in-place
    pub fn modify(file: &PathBuf, field: &str, new_value: &str, stdout: bool) -> Result<()> {
        let mut value = CoreOperations::load_yaml_file(file)?;

        // Parse new value as YAML
        let parsed_value: Value = serde_yaml::from_str(new_value)
            .with_context(|| format!("Failed to parse new value: {}", new_value))?;

        // Set the field
        Self::set_field_value(&mut value, field, parsed_value)?;

        if stdout {
            let yaml = serde_yaml::to_string(&value)?;
            print!("{}", yaml);
        } else {
            CoreOperations::write_yaml_file(file, &value)?;
        }

        Ok(())
    }

    /// Check if field exists and has subfield
    pub fn has_field(file: &PathBuf, field: &str, subfield: &str) -> Result<bool> {
        let value = CoreOperations::load_yaml_file(file)?;

        let target = CoreOperations::get_nested_field(&value, field)?;

        match target {
            Value::Mapping(map) => {
                let subfield_key = Value::String(subfield.to_string());
                Ok(map.contains_key(&subfield_key))
            }
            _ => Ok(false),
        }
    }

    /// Set field value using dot notation
    pub fn set_field_value(value: &mut Value, field: &str, new_value: Value) -> Result<()> {
        let parts: Vec<&str> = field.split('.').collect();
        Self::set_nested_field(value, &parts, new_value)
    }

    // Helper function to set nested field
    fn set_nested_field(value: &mut Value, path: &[&str], new_value: Value) -> Result<()> {
        if path.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }

        if path.len() == 1 {
            // We're at the target field
            match value {
                Value::Mapping(map) => {
                    let key = Value::String(path[0].to_string());
                    map.insert(key, new_value);
                    return Ok(());
                }
                _ => return Err(anyhow::anyhow!("Cannot set field on non-object")),
            }
        }

        // Navigate deeper
        match value {
            Value::Mapping(map) => {
                let key = Value::String(path[0].to_string());
                match map.get_mut(&key) {
                    Some(nested) => Self::set_nested_field(nested, &path[1..], new_value)?,
                    None => {
                        // Create nested structure
                        let mut nested = Value::Mapping(Mapping::new());
                        Self::set_nested_field(&mut nested, &path[1..], new_value)?;
                        map.insert(key, nested);
                    }
                }
            }
            _ => return Err(anyhow::anyhow!("Cannot navigate path on non-object")),
        }

        Ok(())
    }
}
