use serde_yaml::Value;
use serde_yaml_ng as serde_yaml;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use vm_core::error::{Result, VmError};

/// Core YAML operations: file I/O, validation, and basic utilities
pub struct CoreOperations;

impl CoreOperations {
    /// Helper function to read from file or stdin
    pub fn read_file_or_stdin(file: &PathBuf) -> Result<String> {
        if file.to_str() == Some("-") {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(VmError::Io)?;
            Ok(buffer)
        } else {
            fs::read_to_string(file)
                .map_err(|e| VmError::Filesystem(format!("Failed to read file: {file:?}: {e}")))
        }
    }

    /// Validate that a file is valid YAML
    pub fn validate_file(file: &PathBuf) -> Result<()> {
        let content = Self::read_file_or_stdin(file)?;

        let _: Value = serde_yaml::from_str(&content)
            .map_err(|e| VmError::Serialization(format!("Invalid YAML in file: {file:?}: {e}")))?;

        Ok(())
    }

    /// Load YAML file into Value
    pub fn load_yaml_file(file: &PathBuf) -> Result<Value> {
        let content = Self::read_file_or_stdin(file)?;
        serde_yaml::from_str(&content)
            .map_err(|e| VmError::Serialization(format!("Invalid YAML in file: {file:?}: {e}")))
    }

    /// Write Value to YAML file with consistent formatting
    pub fn write_yaml_file(file: &PathBuf, value: &Value) -> Result<()> {
        // Format the value to ensure consistent field ordering
        let formatted_value = super::formatter::format_yaml_value(value)?;

        // Serialize to YAML
        let yaml = serde_yaml::to_string(&formatted_value)?;

        // Post-process for consistent formatting (indentation, etc.)
        let formatted_yaml = super::formatter::post_process_yaml(&yaml);

        // Write to file
        fs::write(file, formatted_yaml)
            .map_err(|e| VmError::Filesystem(format!("Failed to write file: {file:?}: {e}")))
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
