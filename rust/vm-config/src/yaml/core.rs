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

    /// Parse YAML string with enhanced error messages
    ///
    /// This function provides detailed diagnostics for common YAML issues like
    /// duplicate keys, indentation problems, and syntax errors.
    pub fn parse_yaml_with_diagnostics<T>(content: &str, source_description: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_yaml::from_str(content).map_err(|e| {
            let mut message = format!("Failed to parse YAML from {}\n", source_description);
            message.push_str(&format!("Error: {}\n", e));

            // Check for common issues and provide hints
            if let Some(hint) = Self::detect_common_yaml_issues(content, &e) {
                message.push_str(&format!("\nPossible issue: {}\n", hint));
            }

            VmError::Serialization(message)
        })
    }

    /// Detects common YAML issues and provides helpful hints
    fn detect_common_yaml_issues(contents: &str, error: &serde_yaml::Error) -> Option<String> {
        let error_msg = error.to_string();

        // Check for duplicate keys in error message
        if error_msg.contains("duplicate") || error_msg.contains("found duplicate key") {
            return Some(
                "Duplicate key detected in YAML. Each key can only appear once at the same level."
                    .to_string(),
            );
        }

        // Manual check for duplicate keys by scanning the content
        if let Some(duplicate_key) = Self::find_duplicate_keys(contents) {
            return Some(format!(
                "Duplicate key '{}' found. Each key can only appear once at the same level.\n\
                 Please remove or rename the duplicate key.",
                duplicate_key
            ));
        }

        // Check for indentation issues
        if error_msg.contains("indent") || error_msg.contains("indentation") {
            return Some(
                "YAML indentation error. Make sure you're using consistent spaces (not tabs)."
                    .to_string(),
            );
        }

        // Check for invalid syntax
        if error_msg.contains("expected") {
            return Some("YAML syntax error. Check for missing colons, improper quotes, or malformed values.".to_string());
        }

        None
    }

    /// Attempts to find duplicate keys in the YAML content
    fn find_duplicate_keys(contents: &str) -> Option<String> {
        use std::collections::HashMap;

        let mut seen_keys: HashMap<usize, Vec<String>> = HashMap::new();
        let mut indent_stack: Vec<usize> = vec![0];

        for line in contents.lines() {
            // Skip comments and empty lines
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Calculate indentation level
            let indent = line.len() - line.trim_start().len();

            // Adjust indent stack
            while indent_stack.last().is_some_and(|&last| indent < last) {
                indent_stack.pop();
            }
            if indent_stack.last().map_or(true, |&last| indent > last) {
                indent_stack.push(indent);
            }

            // Check if this line defines a key
            let Some(colon_pos) = line.find(':') else {
                continue;
            };

            let key_part = line[..colon_pos].trim();
            // Skip if it's a list item or empty
            if key_part.starts_with('-') || key_part.is_empty() {
                continue;
            }

            let current_indent = *indent_stack.last().unwrap_or(&0);
            let keys_at_level = seen_keys.entry(current_indent).or_default();

            if keys_at_level.contains(&key_part.to_string()) {
                return Some(key_part.to_string());
            }
            keys_at_level.push(key_part.to_string());
        }

        None
    }

    /// Validate that a file is valid YAML
    pub fn validate_file(file: &PathBuf) -> Result<()> {
        let content = Self::read_file_or_stdin(file)?;
        let source_desc = format!("file: {file:?}");
        let _: Value = Self::parse_yaml_with_diagnostics(&content, &source_desc)?;
        Ok(())
    }

    /// Load YAML file into Value
    pub fn load_yaml_file(file: &PathBuf) -> Result<Value> {
        let content = Self::read_file_or_stdin(file)?;
        let source_desc = format!("file: {file:?}");
        Self::parse_yaml_with_diagnostics(&content, &source_desc)
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
