use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use vm_plugin::{discover_plugins, get_preset_plugins, get_service_plugins, PluginType};

pub fn handle_plugin_list() -> Result<()> {
    let plugins = discover_plugins()?;

    if plugins.is_empty() {
        println!("No plugins installed.");
        println!();
        println!("To install a plugin:");
        println!("  vm plugin install <path-to-plugin>");
        println!();
        println!("To create a new plugin:");
        println!("  vm plugin new <plugin-name> --type <preset|service>");
        return Ok(());
    }

    println!("Installed plugins:");
    println!();

    let preset_plugins = get_preset_plugins(&plugins);
    let service_plugins = get_service_plugins(&plugins);

    if !preset_plugins.is_empty() {
        println!("Presets:");
        for plugin in preset_plugins {
            println!("  {} (v{})", plugin.info.name, plugin.info.version);
            if let Some(desc) = &plugin.info.description {
                println!("    {}", desc);
            }
            if let Some(author) = &plugin.info.author {
                println!("    Author: {}", author);
            }
            println!();
        }
    }

    if !service_plugins.is_empty() {
        println!("Services:");
        for plugin in service_plugins {
            println!("  {} (v{})", plugin.info.name, plugin.info.version);
            if let Some(desc) = &plugin.info.description {
                println!("    {}", desc);
            }
            if let Some(author) = &plugin.info.author {
                println!("    Author: {}", author);
            }
            println!();
        }
    }

    Ok(())
}

pub fn handle_plugin_info(plugin_name: &str) -> Result<()> {
    let plugins = discover_plugins()?;

    let plugin = plugins
        .iter()
        .find(|p| p.info.name == plugin_name)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_name))?;

    println!("Plugin: {}", plugin.info.name);
    println!("Version: {}", plugin.info.version);
    println!("Type: {:?}", plugin.info.plugin_type);

    if let Some(desc) = &plugin.info.description {
        println!("Description: {}", desc);
    }

    if let Some(author) = &plugin.info.author {
        println!("Author: {}", author);
    }

    println!();
    println!("Content file: {}", plugin.content_file.display());

    // Load and display content details
    match plugin.info.plugin_type {
        PluginType::Preset => {
            if let Ok(content) = vm_plugin::load_preset_content(plugin) {
                println!();
                println!("Preset Details:");
                if !content.packages.is_empty() {
                    println!("  Packages: {}", content.packages.join(", "));
                }
                if !content.npm_packages.is_empty() {
                    println!("  NPM Packages: {}", content.npm_packages.join(", "));
                }
                if !content.pip_packages.is_empty() {
                    println!("  Pip Packages: {}", content.pip_packages.join(", "));
                }
                if !content.cargo_packages.is_empty() {
                    println!("  Cargo Packages: {}", content.cargo_packages.join(", "));
                }
                if !content.services.is_empty() {
                    println!("  Services: {}", content.services.join(", "));
                }
            }
        }
        PluginType::Service => {
            if let Ok(content) = vm_plugin::load_service_content(plugin) {
                println!();
                println!("Service Details:");
                println!("  Image: {}", content.image);
                if !content.ports.is_empty() {
                    println!("  Ports: {}", content.ports.join(", "));
                }
                if !content.volumes.is_empty() {
                    println!("  Volumes: {}", content.volumes.join(", "));
                }
            }
        }
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
    let metadata_path = source.join("plugin.yaml");
    if !metadata_path.exists() {
        anyhow::bail!("Invalid plugin: missing plugin.yaml in {}", source_path);
    }

    // Parse metadata to get plugin name and type
    let metadata_content =
        fs::read_to_string(&metadata_path).context("Failed to read plugin.yaml")?;

    let info: vm_plugin::PluginInfo =
        serde_yaml_ng::from_str(&metadata_content).context("Failed to parse plugin.yaml")?;

    // Verify content file exists
    let content_file = match info.plugin_type {
        PluginType::Preset => "preset.yaml",
        PluginType::Service => "service.yaml",
    };

    if !source.join(content_file).exists() {
        anyhow::bail!(
            "Invalid plugin: missing {} in {}",
            content_file,
            source_path
        );
    }

    // Get plugins directory
    let plugins_base = vm_platform::platform::vm_state_dir()
        .map_err(|e| anyhow::anyhow!("Could not determine VM state directory: {}", e))?
        .join("plugins");

    // Determine target subdirectory based on plugin type
    let target_subdir = match info.plugin_type {
        PluginType::Preset => "presets",
        PluginType::Service => "services",
    };

    let target_dir = plugins_base.join(target_subdir);

    // Create target directory if it doesn't exist
    if !target_dir.exists() {
        fs::create_dir_all(&target_dir).context("Failed to create plugins directory")?;
    }

    let target = target_dir.join(&info.name);

    // Check if plugin already exists
    if target.exists() {
        anyhow::bail!(
            "Plugin '{}' is already installed. Remove it first with: vm plugin remove {}",
            info.name,
            info.name
        );
    }

    // Copy plugin directory
    copy_dir_all(&source, &target).context("Failed to copy plugin files")?;

    println!(
        "✓ Installed {} plugin: {} (v{})",
        match info.plugin_type {
            PluginType::Preset => "preset",
            PluginType::Service => "service",
        },
        info.name,
        info.version
    );

    Ok(())
}

pub fn handle_plugin_remove(plugin_name: &str) -> Result<()> {
    let plugins_base = vm_platform::platform::vm_state_dir()
        .map_err(|e| anyhow::anyhow!("Could not determine VM state directory: {}", e))?
        .join("plugins");

    // Check both presets and services subdirectories
    let preset_path = plugins_base.join("presets").join(plugin_name);
    let service_path = plugins_base.join("services").join(plugin_name);

    if preset_path.exists() {
        fs::remove_dir_all(&preset_path).context("Failed to remove plugin directory")?;
        println!("✓ Removed preset plugin: {}", plugin_name);
        Ok(())
    } else if service_path.exists() {
        fs::remove_dir_all(&service_path).context("Failed to remove plugin directory")?;
        println!("✓ Removed service plugin: {}", plugin_name);
        Ok(())
    } else {
        anyhow::bail!("Plugin '{}' is not installed", plugin_name);
    }
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
