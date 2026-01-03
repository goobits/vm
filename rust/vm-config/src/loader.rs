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

    /// Calculates the relative path from the config file's directory to the current working directory.
    ///
    /// This is useful for SSH commands that want to automatically land in the same
    /// relative directory inside the VM as the user is in on the host.
    ///
    /// # Example
    /// If `vm.yaml` is at `/home/user/myproject/vm.yaml` and the current directory is
    /// `/home/user/myproject/src/components`, this returns `src/components`.
    ///
    /// # Returns
    /// - `Ok(Some(PathBuf))` - The relative path from config dir to cwd
    /// - `Ok(None)` - No config file found, or cwd is not within config dir
    pub fn relative_path_from_config(&self) -> Result<Option<PathBuf>> {
        let current_dir = std::env::current_dir()?;

        // Find the config file location
        let config_path = match self.find_config_path()? {
            Some(path) => path,
            None => return Ok(None),
        };

        // Get the directory containing the config file
        // We need to handle both relative and absolute paths
        let config_dir = if config_path.is_absolute() {
            config_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("/"))
        } else {
            // For relative paths like "vm.yaml", the parent is the current directory
            // For relative paths like "parent/vm.yaml", we need to resolve against cwd
            match config_path.parent() {
                Some(dir) if dir.as_os_str().is_empty() => {
                    // Config is in current directory (e.g., "vm.yaml")
                    current_dir.clone()
                }
                Some(dir) => {
                    // Config is in a relative subdirectory - resolve against cwd
                    current_dir.join(dir)
                }
                None => current_dir.clone(),
            }
        };

        // Canonicalize both paths to handle symlinks consistently
        // If canonicalization fails (e.g., path doesn't exist), fall back to original paths
        let canonical_current = fs::canonicalize(&current_dir).unwrap_or(current_dir);
        let canonical_config_dir = fs::canonicalize(&config_dir).unwrap_or(config_dir);

        // Calculate relative path from config dir to current dir
        match canonical_current.strip_prefix(&canonical_config_dir) {
            Ok(relative) => {
                if relative.as_os_str().is_empty() {
                    // We're in the config directory itself
                    Ok(Some(PathBuf::from(".")))
                } else {
                    Ok(Some(relative.to_path_buf()))
                }
            }
            Err(_) => {
                // Current dir is not within config dir (shouldn't happen normally)
                Ok(None)
            }
        }
    }

    /// Finds the path to the config file without loading it.
    ///
    /// Uses the same priority chain as `load()`:
    /// 1. Current directory `vm.yaml`
    /// 2. Parent directories `vm.yaml`
    /// 3. Global `~/.vm/config.yaml`
    pub fn find_config_path(&self) -> Result<Option<PathBuf>> {
        // Priority 1: Current directory vm.yaml
        let local_config = Path::new("vm.yaml");
        if local_config.exists() {
            return Ok(Some(local_config.to_path_buf()));
        }

        // Priority 2: Walk up directory tree to find vm.yaml
        if let Some(config_path) = self.find_in_parent_dirs("vm.yaml")? {
            return Ok(Some(config_path));
        }

        // Priority 3: Global config
        if let Some(home_dir) = dirs::home_dir() {
            let global_config = home_dir.join(".vm/config.yaml");
            if global_config.exists() {
                return Ok(Some(global_config));
            }
        }

        Ok(None)
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
        use std::sync::OnceLock;

        // Pattern to match unquoted @ values in box field
        // Matches: "box: @value" or "box:@value" (with optional spaces and comments)
        // Captures the @ symbol and the value that follows (alphanumeric, dash, underscore)
        // and preserves any trailing content (like comments)
        static BOX_PATTERN: OnceLock<Regex> = OnceLock::new();
        let re = BOX_PATTERN.get_or_init(|| {
            Regex::new(r"(?m)^(\s*box:\s*)(@[\w-]+)(\s*.*)$")
                .expect("Hardcoded regex pattern should always compile")
        });

        // Replace unquoted @value with '@value', preserving trailing content
        re.replace_all(contents, r#"$1'$2'$3"#).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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

    #[test]
    #[serial]
    fn test_relative_path_from_config_in_nested_dir() {
        use std::io::Write;
        use tempfile::TempDir;

        // Create a temporary directory structure:
        // tempdir/
        //   vm.yaml
        //   src/
        //     components/
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("vm.yaml");
        let nested_dir = temp_dir.path().join("src").join("components");

        // Create vm.yaml
        let mut config_file = std::fs::File::create(&config_path).unwrap();
        config_file
            .write_all(b"version: '2.0'\nprovider: docker\n")
            .unwrap();

        // Create nested directory
        std::fs::create_dir_all(&nested_dir).unwrap();

        // Change to nested directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested_dir).unwrap();

        // Test relative path detection
        let loader = ConfigLoader::new();
        let result = loader.relative_path_from_config();

        // Restore original directory before assertions
        std::env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_ok());
        let relative_path = result.unwrap();
        assert!(relative_path.is_some());
        assert_eq!(
            relative_path.unwrap(),
            PathBuf::from("src").join("components")
        );
    }

    #[test]
    #[serial]
    fn test_relative_path_from_config_at_root() {
        use std::io::Write;
        use tempfile::TempDir;

        // Create a temporary directory with vm.yaml
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("vm.yaml");

        // Create vm.yaml
        let mut config_file = std::fs::File::create(&config_path).unwrap();
        config_file
            .write_all(b"version: '2.0'\nprovider: docker\n")
            .unwrap();

        // Change to the directory containing vm.yaml
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Test relative path detection
        let loader = ConfigLoader::new();
        let result = loader.relative_path_from_config();

        // Restore original directory before assertions
        std::env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_ok());
        let relative_path = result.unwrap();
        assert!(relative_path.is_some());
        assert_eq!(relative_path.unwrap(), PathBuf::from("."));
    }
}
