use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::{PluginManifest, PluginPreset, PluginService};

/// Discovers and loads plugins from the plugins directory
pub struct PluginDiscovery {
    plugins_dir: PathBuf,
    manifests: HashMap<String, PluginManifest>,
}

impl PluginDiscovery {
    /// Creates a new PluginDiscovery instance
    /// Uses ~/.vm/plugins as the default plugins directory
    pub fn new() -> Result<Self> {
        let plugins_dir = vm_platform::platform::vm_state_dir()
            .unwrap_or_else(|_| PathBuf::from(".vm"))
            .join("plugins");

        let mut discovery = Self {
            plugins_dir,
            manifests: HashMap::new(),
        };

        // Load plugins if directory exists
        if discovery.plugins_dir.exists() {
            discovery.load_plugins()?;
        }

        Ok(discovery)
    }

    /// Creates a PluginDiscovery with a custom plugins directory (for testing)
    pub fn with_directory<P: Into<PathBuf>>(plugins_dir: P) -> Result<Self> {
        let plugins_dir = plugins_dir.into();
        let mut discovery = Self {
            plugins_dir,
            manifests: HashMap::new(),
        };

        // Load plugins if directory exists
        if discovery.plugins_dir.exists() {
            discovery.load_plugins()?;
        }

        Ok(discovery)
    }

    /// Loads all plugins from the plugins directory
    fn load_plugins(&mut self) -> Result<()> {
        let entries = fs::read_dir(&self.plugins_dir)
            .with_context(|| format!("Failed to read plugins directory: {:?}", self.plugins_dir))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                if let Err(e) = self.load_plugin(&path) {
                    eprintln!("Warning: Failed to load plugin from {:?}: {}", path, e);
                }
            }
        }

        Ok(())
    }

    /// Loads a single plugin from a directory
    fn load_plugin(&mut self, plugin_dir: &Path) -> Result<()> {
        let manifest_path = plugin_dir.join("plugin.yaml");

        if !manifest_path.exists() {
            return Err(anyhow::anyhow!(
                "Plugin manifest not found: {:?}",
                manifest_path
            ));
        }

        let content = fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read manifest: {:?}", manifest_path))?;

        let manifest: PluginManifest = serde_yaml_ng::from_str(&content)
            .with_context(|| format!("Failed to parse manifest: {:?}", manifest_path))?;

        // Validate plugin name
        if manifest.name.is_empty() {
            return Err(anyhow::anyhow!(
                "Plugin manifest has empty name: {:?}",
                manifest_path
            ));
        }

        self.manifests.insert(manifest.name.clone(), manifest);

        Ok(())
    }

    /// Returns all loaded plugin manifests
    pub fn manifests(&self) -> &HashMap<String, PluginManifest> {
        &self.manifests
    }

    /// Returns a specific plugin manifest by name
    pub fn get_manifest(&self, name: &str) -> Option<&PluginManifest> {
        self.manifests.get(name)
    }

    /// Returns all presets from all plugins
    pub fn get_all_presets(&self) -> Vec<(String, &PluginPreset)> {
        let mut presets = Vec::new();

        for manifest in self.manifests.values() {
            for preset in &manifest.presets {
                presets.push((manifest.name.clone(), preset));
            }
        }

        presets
    }

    /// Returns a specific preset by name (searches all plugins)
    pub fn get_preset(&self, preset_name: &str) -> Option<(String, &PluginPreset)> {
        for manifest in self.manifests.values() {
            for preset in &manifest.presets {
                if preset.name == preset_name {
                    return Some((manifest.name.clone(), preset));
                }
            }
        }
        None
    }

    /// Returns all services from all plugins
    pub fn get_all_services(&self) -> Vec<(String, &PluginService)> {
        let mut services = Vec::new();

        for manifest in self.manifests.values() {
            for service in &manifest.services {
                services.push((manifest.name.clone(), service));
            }
        }

        services
    }

    /// Returns a specific service by name (searches all plugins)
    pub fn get_service(&self, service_name: &str) -> Option<(String, &PluginService)> {
        for manifest in self.manifests.values() {
            for service in &manifest.services {
                if service.name == service_name {
                    return Some((manifest.name.clone(), service));
                }
            }
        }
        None
    }

    /// Returns the plugins directory path
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }
}

impl Default for PluginDiscovery {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            plugins_dir: PathBuf::from(".vm/plugins"),
            manifests: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_plugin(plugins_dir: &Path, name: &str, yaml_content: &str) -> Result<()> {
        let plugin_dir = plugins_dir.join(name);
        fs::create_dir_all(&plugin_dir)?;
        fs::write(plugin_dir.join("plugin.yaml"), yaml_content)?;
        Ok(())
    }

    #[test]
    fn test_discovery_empty_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let discovery = PluginDiscovery::with_directory(temp_dir.path())?;
        assert_eq!(discovery.manifests().len(), 0);
        Ok(())
    }

    #[test]
    fn test_discovery_single_plugin() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let yaml = r#"
name: test-plugin
version: 1.0.0
description: Test plugin
"#;
        create_test_plugin(temp_dir.path(), "test-plugin", yaml)?;

        let discovery = PluginDiscovery::with_directory(temp_dir.path())?;
        assert_eq!(discovery.manifests().len(), 1);
        assert!(discovery.get_manifest("test-plugin").is_some());
        Ok(())
    }

    #[test]
    fn test_discovery_multiple_plugins() -> Result<()> {
        let temp_dir = TempDir::new()?;

        create_test_plugin(
            temp_dir.path(),
            "plugin1",
            "name: plugin1\nversion: 1.0.0\n",
        )?;
        create_test_plugin(
            temp_dir.path(),
            "plugin2",
            "name: plugin2\nversion: 2.0.0\n",
        )?;

        let discovery = PluginDiscovery::with_directory(temp_dir.path())?;
        assert_eq!(discovery.manifests().len(), 2);
        assert!(discovery.get_manifest("plugin1").is_some());
        assert!(discovery.get_manifest("plugin2").is_some());
        Ok(())
    }

    #[test]
    fn test_get_preset() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let yaml = r#"
name: test-plugin
version: 1.0.0
presets:
  - name: nodejs
    base_image: node:20
    packages:
      - curl
  - name: python
    base_image: python:3.11
"#;
        create_test_plugin(temp_dir.path(), "test-plugin", yaml)?;

        let discovery = PluginDiscovery::with_directory(temp_dir.path())?;
        let presets = discovery.get_all_presets();
        assert_eq!(presets.len(), 2);

        let nodejs = discovery.get_preset("nodejs");
        assert!(nodejs.is_some());
        let (plugin_name, preset) = nodejs.unwrap();
        assert_eq!(plugin_name, "test-plugin");
        assert_eq!(preset.name, "nodejs");
        assert_eq!(preset.base_image, "node:20");

        let python = discovery.get_preset("python");
        assert!(python.is_some());

        assert!(discovery.get_preset("nonexistent").is_none());
        Ok(())
    }

    #[test]
    fn test_get_service() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let yaml = r#"
name: test-plugin
version: 1.0.0
services:
  - name: redis
    image: redis:7
    ports:
      - "6379:6379"
  - name: postgres
    image: postgres:15
"#;
        create_test_plugin(temp_dir.path(), "test-plugin", yaml)?;

        let discovery = PluginDiscovery::with_directory(temp_dir.path())?;
        let services = discovery.get_all_services();
        assert_eq!(services.len(), 2);

        let redis = discovery.get_service("redis");
        assert!(redis.is_some());
        let (plugin_name, service) = redis.unwrap();
        assert_eq!(plugin_name, "test-plugin");
        assert_eq!(service.name, "redis");
        assert_eq!(service.image, "redis:7");

        assert!(discovery.get_service("nonexistent").is_none());
        Ok(())
    }

    #[test]
    fn test_invalid_plugin_skipped() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Valid plugin
        create_test_plugin(temp_dir.path(), "valid", "name: valid\nversion: 1.0.0\n")?;

        // Invalid plugin (missing name)
        create_test_plugin(temp_dir.path(), "invalid", "version: 1.0.0\n")?;

        // Discovery should load valid plugin and skip invalid
        let discovery = PluginDiscovery::with_directory(temp_dir.path())?;
        assert_eq!(discovery.manifests().len(), 1);
        assert!(discovery.get_manifest("valid").is_some());
        Ok(())
    }

    #[test]
    fn test_nonexistent_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let nonexistent = temp_dir.path().join("does-not-exist");

        // Should not fail, just return empty discovery
        let discovery = PluginDiscovery::with_directory(&nonexistent)?;
        assert_eq!(discovery.manifests().len(), 0);
        Ok(())
    }
}
