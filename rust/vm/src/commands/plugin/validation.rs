use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use vm_plugin::{validate_plugin_with_context, PluginType, ValidationError, ValidationResult};

pub(super) fn validate_plugin_source(source: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("Plugin source does not exist: {}", source.display()))?;
    if !metadata.file_type().is_dir() {
        anyhow::bail!(
            "Plugin source must be a real directory: {}",
            source.display()
        );
    }
    Ok(())
}

pub(super) fn plugin_from_source(source: &Path) -> Result<vm_plugin::Plugin> {
    validate_plugin_source(source)?;
    let metadata_path = source.join("plugin.yaml");
    require_regular_file(&metadata_path)?;
    let info: vm_plugin::PluginInfo = serde_yaml_ng::from_str(
        &fs::read_to_string(&metadata_path).context("Failed to read plugin.yaml")?,
    )
    .context("Failed to parse plugin.yaml")?;
    let content_file = match info.plugin_type {
        PluginType::Preset => "preset.yaml",
        PluginType::Service => "service.yaml",
    };
    require_regular_file(&source.join(content_file))?;
    validate_plugin_files(source, content_file)?;
    Ok(vm_plugin::Plugin {
        info,
        content_file: source.join(content_file),
    })
}

fn require_regular_file(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        anyhow::bail!("Plugin manifest must be a real file: {}", path.display());
    }
    Ok(())
}

pub(super) fn validate_plugin_files(
    source: &Path,
    content_file: &str,
) -> Result<Vec<&'static str>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Plugin file name is not UTF-8"))?;
        if !entry.file_type()?.is_file() {
            anyhow::bail!("Plugin contains a directory, link, or special file: {name}");
        }
        let known = match name {
            "plugin.yaml" => "plugin.yaml",
            "preset.yaml" if content_file == "preset.yaml" => "preset.yaml",
            "service.yaml" if content_file == "service.yaml" => "service.yaml",
            "README.md" => "README.md",
            _ => anyhow::bail!("Plugin contains unsupported file: {name}"),
        };
        files.push(known);
    }
    if !files.contains(&"plugin.yaml") || !files.contains(&content_file) {
        anyhow::bail!("Plugin is missing plugin.yaml or {content_file}");
    }
    Ok(files)
}

pub(super) fn validate_all(plugin: &vm_plugin::Plugin) -> Result<ValidationResult> {
    let mut result = validate_plugin_with_context(plugin)?;
    if plugin.info.plugin_type == PluginType::Preset && result.is_valid {
        let content = vm_plugin::load_preset_content(plugin)?;
        if let Err(error) = vm_config::validate_plugin_preset_content(&content) {
            result.add_error(ValidationError::new("preset", error.to_string()));
        }
    }
    Ok(result)
}
