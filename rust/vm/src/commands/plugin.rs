use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use vm_cli::msg;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;
use vm_plugin::{
    discover_plugins, get_preset_plugins, get_service_plugins, validate_plugin_with_context,
    PluginType,
};

pub fn handle_plugin_list() -> Result<()> {
    let plugins = discover_plugins()?;

    if plugins.is_empty() {
        vm_println!("{}", MESSAGES.plugin_list_empty);
        return Ok(());
    }

    vm_println!("{}", MESSAGES.plugin_list_header);

    let preset_plugins = get_preset_plugins(&plugins);
    let service_plugins = get_service_plugins(&plugins);

    if !preset_plugins.is_empty() {
        vm_println!("{}", MESSAGES.plugin_list_presets_header);
        for plugin in preset_plugins {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.plugin_list_item,
                    name = &plugin.info.name,
                    version = &plugin.info.version
                )
            );
            if let Some(desc) = &plugin.info.description {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin_list_item_with_desc, description = desc)
                );
            }
            if let Some(author) = &plugin.info.author {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin_list_item_with_author, author = author)
                );
            }
            vm_println!();
        }
    }

    if !service_plugins.is_empty() {
        vm_println!("{}", MESSAGES.plugin_list_services_header);
        for plugin in service_plugins {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.plugin_list_item,
                    name = &plugin.info.name,
                    version = &plugin.info.version
                )
            );
            if let Some(desc) = &plugin.info.description {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin_list_item_with_desc, description = desc)
                );
            }
            if let Some(author) = &plugin.info.author {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin_list_item_with_author, author = author)
                );
            }
            vm_println!();
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

    vm_println!(
        "{}",
        msg!(MESSAGES.plugin_info_name, name = &plugin.info.name)
    );
    vm_println!(
        "{}",
        msg!(MESSAGES.plugin_info_version, version = &plugin.info.version)
    );
    vm_println!(
        "{}",
        msg!(
            MESSAGES.plugin_info_type,
            plugin_type = format!("{:?}", plugin.info.plugin_type)
        )
    );

    if let Some(desc) = &plugin.info.description {
        vm_println!(
            "{}",
            msg!(MESSAGES.plugin_info_description, description = desc)
        );
    }

    if let Some(author) = &plugin.info.author {
        vm_println!("{}", msg!(MESSAGES.plugin_info_author, author = author));
    }

    vm_println!();
    vm_println!(
        "{}",
        msg!(
            MESSAGES.plugin_info_content_file,
            file = plugin.content_file.display().to_string()
        )
    );

    // Load and display content details
    match plugin.info.plugin_type {
        PluginType::Preset => {
            if let Ok(content) = vm_plugin::load_preset_content(plugin) {
                vm_println!("{}", MESSAGES.plugin_info_preset_details_header);
                if !content.packages.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin_info_packages,
                            packages = content.packages.join(", ")
                        )
                    );
                }
                if !content.npm_packages.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin_info_npm_packages,
                            packages = content.npm_packages.join(", ")
                        )
                    );
                }
                if !content.pip_packages.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin_info_pip_packages,
                            packages = content.pip_packages.join(", ")
                        )
                    );
                }
                if !content.cargo_packages.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin_info_cargo_packages,
                            packages = content.cargo_packages.join(", ")
                        )
                    );
                }
                if !content.services.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin_info_services,
                            services = content.services.join(", ")
                        )
                    );
                }
            }
        }
        PluginType::Service => {
            if let Ok(content) = vm_plugin::load_service_content(plugin) {
                vm_println!("{}", MESSAGES.plugin_info_service_details_header);
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin_info_image, image = &content.image)
                );
                if !content.ports.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(MESSAGES.plugin_info_ports, ports = content.ports.join(", "))
                    );
                }
                if !content.volumes.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin_info_volumes,
                            volumes = content.volumes.join(", ")
                        )
                    );
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

    // Create temporary plugin object for validation
    let temp_plugin = vm_plugin::Plugin {
        info: info.clone(),
        content_file: source.join(content_file),
    };

    // Validate plugin before installation
    vm_println!("{}", MESSAGES.plugin_install_validating);
    let validation_result = vm_plugin::validate_plugin(&temp_plugin)?;

    if !validation_result.is_valid {
        vm_println!("{}", MESSAGES.plugin_install_validation_failed);
        for error in &validation_result.errors {
            println!("  ✗ [{}] {}", error.field, error.message);
            if let Some(suggestion) = &error.fix_suggestion {
                println!("    → {}", suggestion);
            }
        }
        println!();
        anyhow::bail!(
            "Cannot install plugin: validation failed with {} errors",
            validation_result.errors.len()
        );
    }

    if !validation_result.warnings.is_empty() {
        println!("⚠ Warnings:");
        for warning in &validation_result.warnings {
            println!("  {}", warning);
        }
        println!();
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

    let plugin_type_str = match info.plugin_type {
        PluginType::Preset => "preset",
        PluginType::Service => "service",
    };

    vm_println!(
        "{}",
        msg!(
            MESSAGES.plugin_install_success,
            r#type = plugin_type_str,
            name = &info.name,
            version = &info.version
        )
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
        vm_println!(
            "{}",
            msg!(MESSAGES.plugin_remove_success_preset, name = plugin_name)
        );
        Ok(())
    } else if service_path.exists() {
        fs::remove_dir_all(&service_path).context("Failed to remove plugin directory")?;
        vm_println!(
            "{}",
            msg!(MESSAGES.plugin_remove_success_service, name = plugin_name)
        );
        Ok(())
    } else {
        anyhow::bail!("Plugin '{}' is not installed", plugin_name);
    }
}

pub fn handle_plugin_validate(plugin_name: &str) -> Result<()> {
    let plugins = discover_plugins()?;

    let plugin = plugins
        .iter()
        .find(|p| p.info.name == plugin_name)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_name))?;

    vm_println!(
        "{}",
        msg!(MESSAGES.plugin_validate_header, name = &plugin.info.name)
    );

    let result = validate_plugin_with_context(plugin)?;

    if result.is_valid {
        vm_println!("{}", MESSAGES.plugin_validate_passed);

        if !result.warnings.is_empty() {
            vm_println!("{}", MESSAGES.plugin_validate_warnings_header);
            for warning in &result.warnings {
                println!("  ⚠ {}", warning);
            }
            println!();
        }

        vm_println!(
            "{}",
            msg!(MESSAGES.plugin_validate_ready, name = &plugin.info.name)
        );
    } else {
        vm_println!("{}", MESSAGES.plugin_validate_failed);

        if !result.errors.is_empty() {
            vm_println!("{}", MESSAGES.plugin_validate_errors_header);
            for error in &result.errors {
                println!("  ✗ [{}] {}", error.field, error.message);
                if let Some(suggestion) = &error.fix_suggestion {
                    println!("    → {}", suggestion);
                }
            }
            println!();
        }

        if !result.warnings.is_empty() {
            vm_println!("{}", MESSAGES.plugin_validate_warnings_header);
            for warning in &result.warnings {
                println!("  ⚠ {}", warning);
            }
            println!();
        }

        anyhow::bail!(
            "Plugin validation failed with {} errors",
            result.errors.len()
        );
    }

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
