//! Shared YAML processing utilities
//!
//! This module provides common YAML file operations to reduce code duplication
//! across the workspace and ensure consistent error handling.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

/// Read and parse a YAML file into the specified type
///
/// # Arguments
/// * `path` - The path to the YAML file to read
///
/// # Returns
/// * `Result<T>` - The parsed data structure or an error
///
/// # Example
/// ```rust
/// use vm_common::yaml_utils::read_yaml_file;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Config {
///     name: String,
///     version: String,
/// }
///
/// // let config: Config = read_yaml_file("config.yaml")?;
/// ```
pub fn read_yaml_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read YAML file: {}", path.display()))?;

    serde_yaml_ng::from_str(&content)
        .with_context(|| format!("Failed to parse YAML file: {}", path.display()))
}

/// Write a data structure to a YAML file
///
/// # Arguments
/// * `path` - The path where to write the YAML file
/// * `data` - The data structure to serialize
///
/// # Returns
/// * `Result<()>` - Success or an error
///
/// # Example
/// ```rust
/// use vm_common::yaml_utils::write_yaml_file;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Config {
///     name: String,
///     version: String,
/// }
///
/// let config = Config {
///     name: "test".to_string(),
///     version: "1.0".to_string(),
/// };
/// // write_yaml_file("config.yaml", &config)?;
/// ```
pub fn write_yaml_file<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    let content =
        serde_yaml_ng::to_string(data).with_context(|| "Failed to serialize data to YAML")?;

    std::fs::write(path, content)
        .with_context(|| format!("Failed to write YAML file: {}", path.display()))
}

/// Parse YAML content from a string
///
/// # Arguments
/// * `content` - The YAML content as a string
///
/// # Returns
/// * `Result<T>` - The parsed data structure or an error
///
/// # Example
/// ```rust
/// use vm_common::yaml_utils::parse_yaml_str;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Config {
///     name: String,
/// }
///
/// let yaml_content = "name: test";
/// let config: Config = parse_yaml_str(yaml_content)?;
/// ```
pub fn parse_yaml_str<T: DeserializeOwned>(content: &str) -> Result<T> {
    serde_yaml_ng::from_str(content).with_context(|| "Failed to parse YAML content")
}

/// Serialize a data structure to YAML string
///
/// # Arguments
/// * `data` - The data structure to serialize
///
/// # Returns
/// * `Result<String>` - The YAML string or an error
pub fn to_yaml_string<T: Serialize>(data: &T) -> Result<String> {
    serde_yaml_ng::to_string(data).with_context(|| "Failed to serialize data to YAML string")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::NamedTempFile;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        count: u32,
    }

    #[test]
    fn test_yaml_file_round_trip() {
        let config = TestConfig {
            name: "test".to_string(),
            count: 42,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // Write to file
        write_yaml_file(path, &config).unwrap();

        // Read back from file
        let loaded_config: TestConfig = read_yaml_file(path).unwrap();

        assert_eq!(config, loaded_config);
    }

    #[test]
    fn test_yaml_string_parsing() {
        let yaml_content = r#"
name: test
count: 42
"#;

        let config: TestConfig = parse_yaml_str(yaml_content).unwrap();

        assert_eq!(config.name, "test");
        assert_eq!(config.count, 42);
    }

    #[test]
    fn test_yaml_string_serialization() {
        let config = TestConfig {
            name: "test".to_string(),
            count: 42,
        };

        let yaml_string = to_yaml_string(&config).unwrap();

        assert!(yaml_string.contains("name: test"));
        assert!(yaml_string.contains("count: 42"));
    }

    #[test]
    fn test_nonexistent_file_error() {
        let result: Result<TestConfig> = read_yaml_file(Path::new("/nonexistent/file.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_yaml_error() {
        let invalid_yaml = "invalid: yaml: content: [unclosed";
        let result: Result<TestConfig> = parse_yaml_str(invalid_yaml);
        assert!(result.is_err());
    }
}
