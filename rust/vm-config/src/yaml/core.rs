use anyhow::{Context, Result};
use serde_yaml_ng as serde_yaml;
use serde_yaml::Value;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

/// Core YAML operations: file I/O, validation, and basic utilities
pub struct CoreOperations;

impl CoreOperations {
    /// Helper function to read from file or stdin
    pub fn read_file_or_stdin(file: &PathBuf) -> Result<String> {
        if file.to_str() == Some("-") {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .with_context(|| "Failed to read from stdin")?;
            Ok(buffer)
        } else {
            fs::read_to_string(file).with_context(|| format!("Failed to read file: {:?}", file))
        }
    }

    /// Validate that a file is valid YAML
    pub fn validate_file(file: &PathBuf) -> Result<()> {
        let content = Self::read_file_or_stdin(file)?;

        let _: Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Invalid YAML in file: {:?}", file))?;

        Ok(())
    }

    /// Load YAML file into Value
    pub fn load_yaml_file(file: &PathBuf) -> Result<Value> {
        let content = Self::read_file_or_stdin(file)?;
        serde_yaml::from_str(&content).with_context(|| format!("Invalid YAML in file: {:?}", file))
    }

    /// Write Value to YAML file
    pub fn write_yaml_file(file: &PathBuf, value: &Value) -> Result<()> {
        let yaml = serde_yaml::to_string(value)?;
        fs::write(file, yaml).with_context(|| format!("Failed to write file: {:?}", file))
    }

    /// Get nested field from YAML value using dot notation
    pub fn get_nested_field<'a>(value: &'a Value, path: &str) -> Result<&'a Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            match current {
                Value::Mapping(map) => {
                    let key = Value::String(part.to_string());
                    current = map
                        .get(&key)
                        .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", part))?;
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Cannot access field '{}' on non-object",
                        part
                    ))
                }
            }
        }

        Ok(current)
    }
}
