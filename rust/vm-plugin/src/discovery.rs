use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::{Plugin, PluginInfo, PluginType, PresetContent, ServiceContent};

/// Discovers plugins from the plugins directory
pub fn discover_plugins() -> Result<Vec<Plugin>> {
    let plugins_dir = vm_platform::platform::vm_state_dir()
        .unwrap_or_else(|_| PathBuf::from(".vm"))
        .join("plugins");

    discover_plugins_in_directory(&plugins_dir)
}

/// Discovers plugins in a specific directory (for testing)
pub fn discover_plugins_in_directory(plugins_dir: &Path) -> Result<Vec<Plugin>> {
    let mut plugins = Vec::new();

    if !plugins_dir.exists() {
        return Ok(plugins);
    }

    // Discover preset plugins
    let presets_dir = plugins_dir.join("presets");
    if presets_dir.exists() {
        for entry in fs::read_dir(&presets_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                match load_plugin(&path, PluginType::Preset) {
                    Ok(plugin) => plugins.push(plugin),
                    Err(e) => {
                        eprintln!("Warning: Failed to load preset plugin from {path:?}: {e}");
                    }
                }
            }
        }
    }

    // Discover service plugins
    let services_dir = plugins_dir.join("services");
    if services_dir.exists() {
        for entry in fs::read_dir(&services_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                match load_plugin(&path, PluginType::Service) {
                    Ok(plugin) => plugins.push(plugin),
                    Err(e) => {
                        eprintln!("Warning: Failed to load service plugin from {path:?}: {e}");
                    }
                }
            }
        }
    }

    Ok(plugins)
}

/// Loads a single plugin from a directory
fn load_plugin(plugin_dir: &Path, expected_type: PluginType) -> Result<Plugin> {
    let info_path = plugin_dir.join("plugin.yaml");

    if !info_path.exists() {
        anyhow::bail!("Plugin metadata not found: {info_path:?}");
    }

    let info_content = fs::read_to_string(&info_path)
        .with_context(|| format!("Failed to read plugin metadata: {info_path:?}"))?;

    let info: PluginInfo = serde_yaml_ng::from_str(&info_content)
        .with_context(|| format!("Failed to parse plugin metadata: {info_path:?}"))?;

    // Validate plugin type matches expected type
    if info.plugin_type != expected_type {
        anyhow::bail!(
            "Plugin type mismatch: expected {:?}, got {:?} for {}",
            expected_type,
            info.plugin_type,
            info.name
        );
    }

    // Determine content file based on plugin type
    let content_file = match info.plugin_type {
        PluginType::Preset => plugin_dir.join("preset.yaml"),
        PluginType::Service => plugin_dir.join("service.yaml"),
    };

    if !content_file.exists() {
        anyhow::bail!("Plugin content file not found: {content_file:?}");
    }

    Ok(Plugin { info, content_file })
}

/// Helper to get presets from discovered plugins
pub fn get_preset_plugins(plugins: &[Plugin]) -> Vec<&Plugin> {
    plugins
        .iter()
        .filter(|p| p.info.plugin_type == PluginType::Preset)
        .collect()
}

/// Helper to get services from discovered plugins
pub fn get_service_plugins(plugins: &[Plugin]) -> Vec<&Plugin> {
    plugins
        .iter()
        .filter(|p| p.info.plugin_type == PluginType::Service)
        .collect()
}

/// Load preset content from a plugin
pub fn load_preset_content(plugin: &Plugin) -> Result<PresetContent> {
    if plugin.info.plugin_type != PluginType::Preset {
        anyhow::bail!("Plugin {} is not a preset plugin", plugin.info.name);
    }

    let content = fs::read_to_string(&plugin.content_file)
        .with_context(|| format!("Failed to read preset content: {:?}", plugin.content_file))?;

    serde_yaml_ng::from_str(&content)
        .with_context(|| format!("Failed to parse preset content: {:?}", plugin.content_file))
}

/// Load service content from a plugin
pub fn load_service_content(plugin: &Plugin) -> Result<ServiceContent> {
    if plugin.info.plugin_type != PluginType::Service {
        anyhow::bail!("Plugin {} is not a service plugin", plugin.info.name);
    }

    let content = fs::read_to_string(&plugin.content_file)
        .with_context(|| format!("Failed to read service content: {:?}", plugin.content_file))?;

    serde_yaml_ng::from_str(&content)
        .with_context(|| format!("Failed to parse service content: {:?}", plugin.content_file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_preset_plugin(plugins_dir: &Path, name: &str) -> Result<()> {
        let plugin_dir = plugins_dir.join("presets").join(name);
        fs::create_dir_all(&plugin_dir)?;

        let info = format!(
            r#"name: {}
version: 1.0.0
description: Test preset plugin
author: Test Author
plugin_type: preset
"#,
            name
        );
        fs::write(plugin_dir.join("plugin.yaml"), info)?;

        let content = r#"packages:
  - curl
  - git
services:
  - postgres
environment:
  TEST_VAR: "test_value"
provision:
  - echo "Setup complete"
"#;
        fs::write(plugin_dir.join("preset.yaml"), content)?;

        Ok(())
    }

    fn create_service_plugin(plugins_dir: &Path, name: &str) -> Result<()> {
        let plugin_dir = plugins_dir.join("services").join(name);
        fs::create_dir_all(&plugin_dir)?;

        let info = format!(
            r#"name: {}
version: 2.0.0
description: Test service plugin
plugin_type: service
"#,
            name
        );
        fs::write(plugin_dir.join("plugin.yaml"), info)?;

        let content = r#"image: redis:7-alpine
ports:
  - "6379:6379"
volumes:
  - "redis_data:/data"
environment:
  REDIS_PASSWORD: secret
"#;
        fs::write(plugin_dir.join("service.yaml"), content)?;

        Ok(())
    }

    #[test]
    fn test_discover_empty_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let plugins = discover_plugins_in_directory(temp_dir.path())?;
        assert_eq!(plugins.len(), 0);
        Ok(())
    }

    #[test]
    fn test_discover_preset_plugin() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_preset_plugin(temp_dir.path(), "rust-advanced")?;

        let plugins = discover_plugins_in_directory(temp_dir.path())?;
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].info.name, "rust-advanced");
        assert_eq!(plugins[0].info.plugin_type, PluginType::Preset);
        assert!(plugins[0].content_file.ends_with("preset.yaml"));
        Ok(())
    }

    #[test]
    fn test_discover_service_plugin() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_service_plugin(temp_dir.path(), "redis-sentinel")?;

        let plugins = discover_plugins_in_directory(temp_dir.path())?;
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].info.name, "redis-sentinel");
        assert_eq!(plugins[0].info.plugin_type, PluginType::Service);
        assert!(plugins[0].content_file.ends_with("service.yaml"));
        Ok(())
    }

    #[test]
    fn test_discover_multiple_plugins() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_preset_plugin(temp_dir.path(), "rust-advanced")?;
        create_preset_plugin(temp_dir.path(), "python-ml")?;
        create_service_plugin(temp_dir.path(), "redis-sentinel")?;

        let plugins = discover_plugins_in_directory(temp_dir.path())?;
        assert_eq!(plugins.len(), 3);

        let preset_plugins = get_preset_plugins(&plugins);
        let service_plugins = get_service_plugins(&plugins);

        assert_eq!(preset_plugins.len(), 2);
        assert_eq!(service_plugins.len(), 1);
        Ok(())
    }

    #[test]
    fn test_load_preset_content() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_preset_plugin(temp_dir.path(), "rust-advanced")?;

        let plugins = discover_plugins_in_directory(temp_dir.path())?;
        let preset = &plugins[0];

        let content = load_preset_content(preset)?;
        assert_eq!(content.packages.len(), 2);
        assert_eq!(content.services.len(), 1);
        assert_eq!(
            content.environment.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_load_service_content() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_service_plugin(temp_dir.path(), "redis-sentinel")?;

        let plugins = discover_plugins_in_directory(temp_dir.path())?;
        let service = &plugins[0];

        let content = load_service_content(service)?;
        assert_eq!(content.image, "redis:7-alpine");
        assert_eq!(content.ports.len(), 1);
        assert_eq!(
            content.environment.get("REDIS_PASSWORD"),
            Some(&"secret".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_invalid_plugin_skipped() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create valid plugin
        create_preset_plugin(temp_dir.path(), "valid")?;

        // Create invalid plugin (missing content file)
        let invalid_dir = temp_dir.path().join("presets").join("invalid");
        fs::create_dir_all(&invalid_dir)?;
        fs::write(
            invalid_dir.join("plugin.yaml"),
            "name: invalid\nversion: 1.0.0\nplugin_type: preset\n",
        )?;
        // Don't create preset.yaml

        let plugins = discover_plugins_in_directory(temp_dir.path())?;
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].info.name, "valid");
        Ok(())
    }

    #[test]
    fn test_nonexistent_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let nonexistent = temp_dir.path().join("does-not-exist");

        let plugins = discover_plugins_in_directory(&nonexistent)?;
        assert_eq!(plugins.len(), 0);
        Ok(())
    }
}
