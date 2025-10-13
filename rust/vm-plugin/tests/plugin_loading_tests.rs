use std::fs;
use std::path::Path;
use tempfile::TempDir;
use vm_plugin::discovery;
use vm_plugin::types::{PluginType};

fn create_preset_plugin(plugins_dir: &Path, name: &str) -> anyhow::Result<()> {
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
"#;
    fs::write(plugin_dir.join("preset.yaml"), content)?;

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_discover_preset_plugin_integration() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    create_preset_plugin(temp_dir.path(), "test-preset")?;

    let plugins = discovery::discover_plugins_in_directory(temp_dir.path())?;
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].info.name, "test-preset");
    assert_eq!(plugins[0].info.plugin_type, PluginType::Preset);
    assert!(plugins[0].content_file.ends_with("preset.yaml"));
    Ok(())
}
