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
        let mut config: VmConfig = serde_yaml_ng::from_str(&contents)
            .with_context(|| format!("Failed to parse YAML from {}", path.display()))?;

        // Store the source path for debugging purposes.
        config.source_path = Some(path.to_path_buf());

        Ok(config)
    }
}
