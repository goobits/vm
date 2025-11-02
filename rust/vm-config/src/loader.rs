// Standard library imports
use std::fs;
use std::path::{Path, PathBuf};

// External crate imports
use anyhow::{bail, Context, Result};
use tracing::debug;

// Internal imports
use crate::config::VmConfig;

/// A loader responsible for finding and loading the `vm.yaml` configuration file.
///
/// The loader implements a clear priority chain for discovering the configuration:
/// 1. **Current Directory:** Looks for `vm.yaml` in the current working directory.
/// 2. **Parent Directories:** Walks up the directory tree from the current directory, looking for `vm.yaml`.
/// 3. **Global Configuration:** As a final fallback, checks for the global config at `~/.vm/config.yaml`.
#[derive(Default)]
pub struct ConfigLoader;

impl ConfigLoader {
    /// Creates a new `ConfigLoader`.
    pub fn new() -> Self {
        Self
    }

    /// Loads the `VmConfig` by searching in the prioritized locations.
    pub fn load(&self) -> Result<VmConfig> {
        // Priority 1: Current directory vm.yaml
        let local_config = Path::new("vm.yaml");
        if local_config.exists() {
            debug!("Loading config from: {}", local_config.display());
            return self
                .load_file(local_config)
                .with_context(|| format!("Failed to load {}", local_config.display()));
        }

        // Priority 2: Walk up directory tree to find vm.yaml
        if let Some(config_path) = self.find_in_parent_dirs("vm.yaml")? {
            debug!("Loading config from: {}", config_path.display());
            return self.load_file(&config_path);
        }

        // Priority 3: Global config
        let home_dir = dirs::home_dir().context("Could not find home directory")?;
        let global_config = home_dir.join(".vm/config.yaml");
        if global_config.exists() {
            debug!("Loading config from: {}", global_config.display());
            return self.load_file(&global_config);
        }

        bail!("No vm.yaml found in current directory or parent directories. Please run `vm init` or create a vm.yaml file.");
    }

    /// Finds a file by walking up the directory tree from the current directory.
    fn find_in_parent_dirs(&self, filename: &str) -> Result<Option<PathBuf>> {
        let current_dir = std::env::current_dir()?;
        let mut current = current_dir.as_path();

        loop {
            let config_path = current.join(filename);
            if config_path.exists() {
                return Ok(Some(config_path));
            }

            match current.parent() {
                Some(parent) => current = parent,
                None => break, // Reached the root
            }
        }

        Ok(None)
    }

    /// Loads and deserializes a `VmConfig` from a given file path.
    ///
    /// It also injects the source path of the file into the configuration object
    /// for debugging and display purposes.
    fn load_file(&self, path: &Path) -> Result<VmConfig> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read file at {}", path.display()))?;

        // Preprocess to handle unquoted @ symbols in box field
        let preprocessed = Self::preprocess_yaml(&contents);

        // Use centralized YAML parsing with enhanced diagnostics
        let source_desc = format!("{}", path.display());
        let mut config: VmConfig =
            crate::yaml::CoreOperations::parse_yaml_with_diagnostics(&preprocessed, &source_desc)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Store the source path for debugging purposes.
        config.source_path = Some(path.to_path_buf());

        Ok(config)
    }

    /// Preprocesses YAML content to handle special cases like unquoted @ symbols.
    ///
    /// This allows users to write `box: @snapshot-name` without quotes,
    /// which is more ergonomic than requiring `box: '@snapshot-name'`.
    fn preprocess_yaml(contents: &str) -> String {
        use regex::Regex;

        // Pattern to match unquoted @ values in box field
        // Matches: "box: @value" or "box:@value" (with optional spaces and comments)
        // Captures the @ symbol and the value that follows (alphanumeric, dash, underscore)
        // and preserves any trailing content (like comments)
        let re = Regex::new(r"(?m)^(\s*box:\s*)(@[\w-]+)(\s*.*)$").unwrap();

        // Replace unquoted @value with '@value', preserving trailing content
        re.replace_all(contents, r#"$1'$2'$3"#).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_yaml_unquoted_snapshot() {
        let input = r#"
version: '2.0'
vm:
  box: @vibe-base
  memory: 4096
"#;
        let expected = r#"
version: '2.0'
vm:
  box: '@vibe-base'
  memory: 4096
"#;
        let result = ConfigLoader::preprocess_yaml(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_preprocess_yaml_already_quoted() {
        let input = r#"
version: '2.0'
vm:
  box: '@vibe-base'
  memory: 4096
"#;
        // Should not double-quote
        let result = ConfigLoader::preprocess_yaml(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_preprocess_yaml_no_spaces() {
        let input = r#"vm:
  box:@snapshot-name"#;
        let expected = r#"vm:
  box:'@snapshot-name'"#;
        let result = ConfigLoader::preprocess_yaml(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_preprocess_yaml_with_comment() {
        let input = r#"vm:
  box: @vibe-base  # my snapshot"#;
        let expected = r#"vm:
  box: '@vibe-base'  # my snapshot"#;
        let result = ConfigLoader::preprocess_yaml(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_preprocess_yaml_preserves_other_fields() {
        let input = r#"
version: '2.0'
provider: docker
vm:
  box: @snapshot
  memory: unlimited
services:
  postgresql:
    enabled: true
"#;
        let result = ConfigLoader::preprocess_yaml(input);
        assert!(result.contains("box: '@snapshot'"));
        assert!(result.contains("version: '2.0'"));
        assert!(result.contains("memory: unlimited"));
        assert!(result.contains("postgresql:"));
    }

    #[test]
    fn test_load_config_with_unquoted_snapshot() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a temporary config file with unquoted @ snapshot
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"version: '2.0'
provider: docker
vm:
  box: @vibe-base
  memory: 4096
"#;
        temp_file.write_all(config_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        // Load the config file
        let loader = ConfigLoader::new();
        let result = loader.load_file(temp_file.path());

        // Should successfully load without errors
        assert!(result.is_ok());
        let config = result.unwrap();

        // Verify the box value was parsed correctly
        assert_eq!(
            config.vm.and_then(|v| v.r#box).map(|b| match b {
                crate::config::BoxSpec::String(s) => s,
                _ => panic!("Expected BoxSpec::String"),
            }),
            Some("@vibe-base".to_string())
        );
    }

    #[test]
    fn test_load_config_with_quoted_snapshot() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a temporary config file with quoted @ snapshot
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"version: '2.0'
provider: docker
vm:
  box: '@vibe-base'
  memory: 4096
"#;
        temp_file.write_all(config_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        // Load the config file
        let loader = ConfigLoader::new();
        let result = loader.load_file(temp_file.path());

        // Should successfully load without errors
        assert!(result.is_ok());
        let config = result.unwrap();

        // Verify the box value was parsed correctly
        assert_eq!(
            config.vm.and_then(|v| v.r#box).map(|b| match b {
                crate::config::BoxSpec::String(s) => s,
                _ => panic!("Expected BoxSpec::String"),
            }),
            Some("@vibe-base".to_string())
        );
    }
}
