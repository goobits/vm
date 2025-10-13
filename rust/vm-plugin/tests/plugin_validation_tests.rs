#[cfg(feature = "integration")]
use std::fs;
#[cfg(feature = "integration")]
use std::path::Path;
#[cfg(feature = "integration")]
use tempfile::TempDir;
use vm_plugin::types::{Plugin, PluginInfo, PluginType};
#[cfg(feature = "integration")]
use vm_plugin::validation;

#[cfg(feature = "integration")]
fn create_test_plugin(
    plugins_dir: &Path,
    name: &str,
    plugin_type: PluginType,
) -> anyhow::Result<Plugin> {
    let (type_str, content_file, content) = match plugin_type {
        PluginType::Preset => ("preset", "preset.yaml", "packages:\n  - git\n"),
        PluginType::Service => ("service", "service.yaml", "image: alpine:latest\n"),
    };

    let plugin_dir = plugins_dir.join(format!("{}s", type_str)).join(name);
    fs::create_dir_all(&plugin_dir)?;

    let info = PluginInfo {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: Some(format!("A test {} plugin.", type_str)),
        author: Some("Test".to_string()),
        plugin_type,
    };
    fs::write(
        plugin_dir.join("plugin.yaml"),
        serde_yaml_ng::to_string(&info)?,
    )?;
    fs::write(plugin_dir.join(content_file), content)?;

    Ok(Plugin {
        info,
        content_file: plugin_dir.join(content_file),
    })
}

#[test]
#[cfg(feature = "integration")]
fn test_valid_plugin_validation() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let plugin = create_test_plugin(temp_dir.path(), "valid-plugin", PluginType::Preset)?;

    let result = validation::validate_plugin(&plugin)?;
    assert!(result.is_valid, "Validation should pass for a valid plugin");
    assert!(
        result.errors.is_empty(),
        "There should be no errors for a valid plugin"
    );

    Ok(())
}

#[test]
#[cfg(feature = "integration")]
fn test_invalid_plugin_name_validation() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let mut plugin = create_test_plugin(temp_dir.path(), "invalid-plugin", PluginType::Preset)?;
    plugin.info.name = "invalid name".to_string(); // Invalid character

    let result = validation::validate_plugin(&plugin)?;
    assert!(
        !result.is_valid,
        "Validation should fail for a plugin with an invalid name"
    );
    assert!(
        result.errors.iter().any(|e| e.field == "name"),
        "An error for the 'name' field should be reported"
    );

    Ok(())
}
