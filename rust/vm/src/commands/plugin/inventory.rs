use anyhow::Result;
use std::path::Path;
use vm_core::{msg, vm_println};
use vm_messages::messages::MESSAGES;
use vm_plugin::{discover_plugins, get_preset_plugins, get_service_plugins, PluginType};

use super::validation::{plugin_from_source, validate_all};

pub(super) fn handle_plugin_list() -> Result<()> {
    let plugins = discover_plugins()?;

    if plugins.is_empty() {
        vm_println!("{}", MESSAGES.plugin.list_empty);
        return Ok(());
    }

    vm_println!("{}", MESSAGES.plugin.list_header);

    let preset_plugins = get_preset_plugins(&plugins);
    let service_plugins = get_service_plugins(&plugins);

    if !preset_plugins.is_empty() {
        vm_println!("{}", MESSAGES.plugin.list_presets_header);
        for plugin in preset_plugins {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.plugin.list_item,
                    name = &plugin.info.name,
                    version = &plugin.info.version
                )
            );
            if let Some(desc) = &plugin.info.description {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin.list_item_with_desc, description = desc)
                );
            }
            if let Some(author) = &plugin.info.author {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin.list_item_with_author, author = author)
                );
            }
            vm_println!();
        }
    }

    if !service_plugins.is_empty() {
        vm_println!("{}", MESSAGES.plugin.list_services_header);
        for plugin in service_plugins {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.plugin.list_item,
                    name = &plugin.info.name,
                    version = &plugin.info.version
                )
            );
            if let Some(desc) = &plugin.info.description {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin.list_item_with_desc, description = desc)
                );
            }
            if let Some(author) = &plugin.info.author {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin.list_item_with_author, author = author)
                );
            }
            vm_println!();
        }
    }

    Ok(())
}

pub(super) fn handle_plugin_info(plugin_name: &str) -> Result<()> {
    let plugins = discover_plugins()?;

    let plugin = plugins
        .iter()
        .find(|p| p.info.name == plugin_name)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{plugin_name}' not found"))?;

    vm_println!(
        "{}",
        msg!(MESSAGES.plugin.info_name, name = &plugin.info.name)
    );
    vm_println!(
        "{}",
        msg!(MESSAGES.plugin.info_version, version = &plugin.info.version)
    );
    vm_println!(
        "{}",
        msg!(
            MESSAGES.plugin.info_type,
            plugin_type = format!("{:?}", plugin.info.plugin_type)
        )
    );

    if let Some(desc) = &plugin.info.description {
        vm_println!(
            "{}",
            msg!(MESSAGES.plugin.info_description, description = desc)
        );
    }

    if let Some(author) = &plugin.info.author {
        vm_println!("{}", msg!(MESSAGES.plugin.info_author, author = author));
    }

    vm_println!();
    vm_println!(
        "{}",
        msg!(
            MESSAGES.plugin.info_content_file,
            file = plugin.content_file.display().to_string()
        )
    );

    // Load and display content details
    match plugin.info.plugin_type {
        PluginType::Preset => {
            if let Ok(content) = vm_plugin::load_preset_content(plugin) {
                vm_println!("{}", MESSAGES.plugin.info_preset_details_header);
                if !content.packages.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin.info_packages,
                            packages = content.packages.join(", ")
                        )
                    );
                }
                if !content.npm_packages.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin.info_npm_packages,
                            packages = content.npm_packages.join(", ")
                        )
                    );
                }
                if !content.pip_packages.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin.info_pip_packages,
                            packages = content.pip_packages.join(", ")
                        )
                    );
                }
                if !content.cargo_packages.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin.info_cargo_packages,
                            packages = content.cargo_packages.join(", ")
                        )
                    );
                }
                if !content.services.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin.info_services,
                            services = content.services.join(", ")
                        )
                    );
                }
            }
        }
        PluginType::Service => {
            if let Ok(content) = vm_plugin::load_service_content(plugin) {
                vm_println!("{}", MESSAGES.plugin.info_service_details_header);
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin.info_image, image = &content.image)
                );
                if !content.ports.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(MESSAGES.plugin.info_ports, ports = content.ports.join(", "))
                    );
                }
                if !content.volumes.is_empty() {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin.info_volumes,
                            volumes = content.volumes.join(", ")
                        )
                    );
                }
            }
        }
    }

    Ok(())
}

pub(super) fn handle_plugin_validate(plugin_name: &str) -> Result<()> {
    let path = Path::new(plugin_name);
    let source_plugin;
    let installed_plugins;
    let plugin = if path.exists() || path.components().count() > 1 {
        source_plugin = plugin_from_source(path)?;
        &source_plugin
    } else {
        installed_plugins = discover_plugins()?;
        installed_plugins
            .iter()
            .find(|plugin| plugin.info.name == plugin_name)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{plugin_name}' not found"))?
    };

    vm_println!(
        "{}",
        msg!(MESSAGES.plugin.validate_header, name = &plugin.info.name)
    );

    let result = validate_all(plugin)?;

    if result.is_valid {
        vm_println!("{}", MESSAGES.plugin.validate_passed);

        if !result.warnings.is_empty() {
            vm_println!("{}", MESSAGES.plugin.validate_warnings_header);
            for warning in &result.warnings {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin.validate_warning_item, warning = warning)
                );
            }
            vm_println!();
        }

        vm_println!(
            "{}",
            msg!(MESSAGES.plugin.validate_ready, name = &plugin.info.name)
        );
    } else {
        vm_println!("{}", MESSAGES.plugin.validate_failed);

        if !result.errors.is_empty() {
            vm_println!("{}", MESSAGES.plugin.validate_errors_header);
            for error in &result.errors {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.plugin.validate_error_item,
                        field = &error.field,
                        message = &error.message
                    )
                );
                if let Some(suggestion) = &error.fix_suggestion {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.plugin.validate_error_suggestion,
                            suggestion = suggestion
                        )
                    );
                }
            }
            vm_println!();
        }

        if !result.warnings.is_empty() {
            vm_println!("{}", MESSAGES.plugin.validate_warnings_header);
            for warning in &result.warnings {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.plugin.validate_warning_item, warning = warning)
                );
            }
            vm_println!();
        }

        anyhow::bail!(
            "Plugin validation failed with {} errors",
            result.errors.len()
        );
    }

    Ok(())
}
