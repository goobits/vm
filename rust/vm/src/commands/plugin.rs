use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use vm_plugin::PluginDiscovery;

pub fn handle_plugin_list() -> Result<()> {
    let discovery = PluginDiscovery::new()?;
    let manifests = discovery.manifests();

    if manifests.is_empty() {
        println!("No plugins installed.");
        println!();
        println!("To install a plugin:");
        println!("  vm plugin install <path-to-plugin>");
        println!();
        println!("To create a new plugin:");
        println!("  vm plugin new <plugin-name>");
        return Ok(());
    }

    println!("Installed plugins:");
    println!();

    for (name, manifest) in manifests {
        println!("  {} (v{})", name, manifest.version);
        if let Some(desc) = &manifest.description {
            println!("    {}", desc);
        }
        if let Some(author) = &manifest.author {
            println!("    Author: {}", author);
        }

        if !manifest.presets.is_empty() {
            println!("    Presets: {}", manifest.presets.len());
        }
        if !manifest.services.is_empty() {
            println!("    Services: {}", manifest.services.len());
        }
        println!();
    }

    Ok(())
}

pub fn handle_plugin_info(plugin_name: &str) -> Result<()> {
    let discovery = PluginDiscovery::new()?;

    let manifest = discovery
        .get_manifest(plugin_name)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_name))?;

    println!("Plugin: {}", manifest.name);
    println!("Version: {}", manifest.version);

    if let Some(desc) = &manifest.description {
        println!("Description: {}", desc);
    }

    if let Some(author) = &manifest.author {
        println!("Author: {}", author);
    }

    println!();

    if !manifest.presets.is_empty() {
        println!("Presets ({}):", manifest.presets.len());
        for preset in &manifest.presets {
            println!("  - {}", preset.name);
            if let Some(desc) = &preset.description {
                println!("      {}", desc);
            }
            println!("      Base: {}", preset.base_image);
        }
        println!();
    }

    if !manifest.services.is_empty() {
        println!("Services ({}):", manifest.services.len());
        for service in &manifest.services {
            println!("  - {}", service.name);
            if let Some(desc) = &service.description {
                println!("      {}", desc);
            }
            println!("      Image: {}", service.image);
        }
        println!();
    }

    Ok(())
}

pub fn handle_plugin_install(source_path: &str) -> Result<()> {
    let source = PathBuf::from(source_path);

    if !source.exists() {
        anyhow::bail!("Plugin source path does not exist: {}", source_path);
    }

    if !source.is_dir() {
        anyhow::bail!("Plugin source must be a directory: {}", source_path);
    }

    // Verify plugin.yaml exists
    let manifest_path = source.join("plugin.yaml");
    if !manifest_path.exists() {
        anyhow::bail!("Invalid plugin: missing plugin.yaml in {}", source_path);
    }

    // Parse manifest to get plugin name
    let manifest_content =
        fs::read_to_string(&manifest_path).context("Failed to read plugin.yaml")?;

    let manifest: vm_plugin::PluginManifest =
        serde_yaml_ng::from_str(&manifest_content).context("Failed to parse plugin.yaml")?;

    // Get plugins directory
    let plugins_base = vm_platform::platform::vm_state_dir()
        .map_err(|e| anyhow::anyhow!("Could not determine VM state directory: {}", e))?
        .join("plugins");

    // Create plugins directory if it doesn't exist
    if !plugins_base.exists() {
        fs::create_dir_all(&plugins_base).context("Failed to create plugins directory")?;
    }

    let target = plugins_base.join(&manifest.name);

    // Check if plugin already exists
    if target.exists() {
        anyhow::bail!(
            "Plugin '{}' is already installed. Remove it first with: vm plugin remove {}",
            manifest.name,
            manifest.name
        );
    }

    // Copy plugin directory
    copy_dir_all(&source, &target).context("Failed to copy plugin files")?;

    println!(
        "✓ Installed plugin: {} (v{})",
        manifest.name, manifest.version
    );

    if !manifest.presets.is_empty() {
        println!("  Presets: {}", manifest.presets.len());
    }
    if !manifest.services.is_empty() {
        println!("  Services: {}", manifest.services.len());
    }

    Ok(())
}

pub fn handle_plugin_remove(plugin_name: &str) -> Result<()> {
    let plugins_base = vm_platform::platform::vm_state_dir()
        .map_err(|e| anyhow::anyhow!("Could not determine VM state directory: {}", e))?
        .join("plugins");

    let plugin_dir = plugins_base.join(plugin_name);

    if !plugin_dir.exists() {
        anyhow::bail!("Plugin '{}' is not installed", plugin_name);
    }

    fs::remove_dir_all(&plugin_dir).context("Failed to remove plugin directory")?;

    println!("✓ Removed plugin: {}", plugin_name);

    Ok(())
}

// Helper function to recursively copy directories
fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}
