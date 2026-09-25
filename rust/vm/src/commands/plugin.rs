use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use vm_core::msg;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;
use vm_plugin::{
    discover_plugins, get_preset_plugins, get_service_plugins, is_valid_plugin_name,
    validate_plugin_with_context, PluginType,
};

use crate::cli::PluginSubcommand;
use crate::error::{VmError, VmResult};

pub(super) fn handle_command(command: &PluginSubcommand) -> VmResult<()> {
    match command {
        PluginSubcommand::List => handle_plugin_list().map_err(VmError::from),
        PluginSubcommand::Show { plugin_name } => {
            handle_plugin_info(plugin_name).map_err(VmError::from)
        }
        PluginSubcommand::Install { source_path } => {
            handle_plugin_install(source_path).map_err(VmError::from)
        }
        PluginSubcommand::Remove { plugin_name } => {
            handle_plugin_remove(plugin_name).map_err(VmError::from)
        }
        PluginSubcommand::Create { plugin_name, kind } => {
            super::plugin_new::handle_plugin_new(plugin_name, kind).map_err(VmError::from)
        }
        PluginSubcommand::Validate { plugin_name } => {
            handle_plugin_validate(plugin_name).map_err(VmError::from)
        }
    }
}

fn handle_plugin_list() -> Result<()> {
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

fn handle_plugin_info(plugin_name: &str) -> Result<()> {
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

fn handle_plugin_install(source_path: &str) -> Result<()> {
    let source = PathBuf::from(source_path);
    let temp_plugin = plugin_from_source(&source)?;
    let info = &temp_plugin.info;
    let content_file = match info.plugin_type {
        PluginType::Preset => "preset.yaml",
        PluginType::Service => "service.yaml",
    };

    // Validate plugin before installation
    vm_println!("{}", MESSAGES.plugin.install_validating);
    let validation_result = validate_plugin_with_context(&temp_plugin)?;

    if !validation_result.is_valid {
        vm_println!("{}", MESSAGES.plugin.install_validation_failed);
        for error in &validation_result.errors {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.plugin.install_validation_error,
                    field = &error.field,
                    message = &error.message
                )
            );
            if let Some(suggestion) = &error.fix_suggestion {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.plugin.install_validation_error_with_suggestion,
                        suggestion = suggestion
                    )
                );
            }
        }
        vm_println!();
        anyhow::bail!(
            "Cannot install plugin: validation failed with {} errors",
            validation_result.errors.len()
        );
    }

    if !validation_result.warnings.is_empty() {
        vm_println!("{}", MESSAGES.plugin.install_warnings_header);
        for warning in &validation_result.warnings {
            vm_println!(
                "{}",
                msg!(MESSAGES.plugin.install_warning_item, warning = warning)
            );
        }
        vm_println!();
    }

    // Get plugins directory
    let plugins_base = vm_platform::platform::vm_state_dir()
        .map_err(|e| anyhow::anyhow!("Could not determine VM state directory: {e}"))?
        .join("plugins");

    // Determine target subdirectory based on plugin type
    let target_subdir = match info.plugin_type {
        PluginType::Preset => "presets",
        PluginType::Service => "services",
    };

    install_validated_plugin(&source, &plugins_base, target_subdir, info, content_file)?;

    let plugin_type_str = match info.plugin_type {
        PluginType::Preset => "preset",
        PluginType::Service => "service",
    };

    vm_println!(
        "{}",
        msg!(
            MESSAGES.plugin.install_success,
            r#type = plugin_type_str,
            name = &info.name,
            version = &info.version
        )
    );

    Ok(())
}

fn handle_plugin_remove(plugin_name: &str) -> Result<()> {
    if !is_valid_plugin_name(plugin_name) {
        anyhow::bail!(
            "Invalid plugin name '{plugin_name}': use only letters, numbers, hyphens, and underscores"
        );
    }

    let plugins_base = vm_platform::platform::vm_state_dir()
        .map_err(|e| anyhow::anyhow!("Could not determine VM state directory: {e}"))?
        .join("plugins");
    require_real_directory(plugins_base.parent().context("Invalid plugin state path")?)?;
    require_real_directory(&plugins_base)?;
    let _lock = lock_plugin_changes(&plugins_base)?;

    // Check both presets and services subdirectories
    let preset_path = plugins_base.join("presets").join(plugin_name);
    let service_path = plugins_base.join("services").join(plugin_name);

    let preset_exists = fs::symlink_metadata(&preset_path).is_ok();
    let service_exists = fs::symlink_metadata(&service_path).is_ok();
    if preset_exists && service_exists {
        anyhow::bail!(
            "Plugin '{plugin_name}' exists in both plugin groups; inspect storage before removal"
        );
    }

    if preset_exists {
        require_real_directory(&plugins_base.join("presets"))?;
        require_real_directory(&preset_path)?;
        fs::remove_dir_all(&preset_path).context("Failed to remove plugin directory")?;
        vm_println!(
            "{}",
            msg!(MESSAGES.plugin.remove_success_preset, name = plugin_name)
        );
        Ok(())
    } else if service_exists {
        require_real_directory(&plugins_base.join("services"))?;
        require_real_directory(&service_path)?;
        fs::remove_dir_all(&service_path).context("Failed to remove plugin directory")?;
        vm_println!(
            "{}",
            msg!(MESSAGES.plugin.remove_success_service, name = plugin_name)
        );
        Ok(())
    } else {
        anyhow::bail!("Plugin '{plugin_name}' is not installed");
    }
}

fn require_real_directory(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
        anyhow::bail!("Plugin path is not a real directory: {}", path.display());
    }
    Ok(())
}

fn lock_plugin_changes(plugins_base: &Path) -> Result<fs::File> {
    let path = plugins_base.join(".install.lock");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => anyhow::bail!("Plugin lock path is not a regular file"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)?;
    fs2::FileExt::lock_exclusive(&file)?;
    Ok(file)
}

fn handle_plugin_validate(plugin_name: &str) -> Result<()> {
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

    let result = validate_plugin_with_context(plugin)?;

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

fn validate_plugin_source(source: &Path) -> Result<()> {
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

fn plugin_from_source(source: &Path) -> Result<vm_plugin::Plugin> {
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

fn validate_plugin_files(source: &Path, content_file: &str) -> Result<Vec<&'static str>> {
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

fn ensure_real_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => anyhow::bail!(
            "Plugin installation path is not a real directory: {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).with_context(|| format!("Failed to create {}", path.display()))
        }
        Err(error) => Err(error).with_context(|| format!("Failed to inspect {}", path.display())),
    }
}

fn install_validated_plugin(
    source: &Path,
    plugins_base: &Path,
    target_subdir: &str,
    info: &vm_plugin::PluginInfo,
    content_file: &str,
) -> Result<()> {
    if !is_valid_plugin_name(&info.name) {
        anyhow::bail!("Invalid plugin name '{}'", info.name);
    }
    let state_dir = plugins_base.parent().context("Invalid plugins directory")?;
    ensure_real_directory(state_dir)?;
    ensure_real_directory(plugins_base)?;
    let _lock = lock_plugin_changes(plugins_base)?;
    let target_dir = plugins_base.join(target_subdir);
    ensure_real_directory(&target_dir)?;
    for kind in ["presets", "services"] {
        let existing = plugins_base.join(kind).join(&info.name);
        if fs::symlink_metadata(&existing).is_ok() {
            anyhow::bail!("Plugin '{}' is already installed", info.name);
        }
    }
    if info.plugin_type == PluginType::Preset
        && vm_config::PresetDetector::new(PathBuf::new())
            .list_all_presets()?
            .contains(&info.name)
    {
        anyhow::bail!("Plugin '{}' conflicts with an existing preset", info.name);
    }

    let stage = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(plugins_base)?;
    let payload = stage.path().join("payload");
    fs::create_dir(&payload)?;
    for file in validate_plugin_files(source, content_file)? {
        fs::copy(source.join(file), payload.join(file))
            .with_context(|| format!("Failed to stage {file}"))?;
        restrict_file_permissions(&payload.join(file))?;
    }
    restrict_directory_permissions(&payload)?;
    let staged_plugin = vm_plugin::Plugin {
        info: info.clone(),
        content_file: payload.join(content_file),
    };
    let staged_info: vm_plugin::PluginInfo =
        serde_yaml_ng::from_str(&fs::read_to_string(payload.join("plugin.yaml"))?)?;
    if staged_info.name != info.name
        || staged_info.version != info.version
        || staged_info.description != info.description
        || staged_info.author != info.author
        || staged_info.plugin_type != info.plugin_type
        || staged_info.preset_category != info.preset_category
    {
        anyhow::bail!("Plugin metadata changed during installation");
    }
    let staged_validation = validate_plugin_with_context(&staged_plugin)?;
    if !staged_validation.is_valid {
        anyhow::bail!("Plugin content changed during installation and no longer validates");
    }

    let target = target_dir.join(&info.name);
    if fs::symlink_metadata(&target).is_ok() {
        anyhow::bail!("Plugin '{}' is already installed", info.name);
    }
    fs::rename(&payload, &target).context("Failed to activate staged plugin")?;
    Ok(())
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        handle_plugin_validate, install_validated_plugin, validate_plugin_files,
        validate_plugin_source,
    };
    use std::fs;
    use vm_plugin::{PluginInfo, PluginType};

    fn preset_info() -> PluginInfo {
        PluginInfo {
            name: "unique-test-preset-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test preset".to_string()),
            author: None,
            plugin_type: PluginType::Preset,
            preset_category: None,
        }
    }

    #[test]
    fn plugin_install_stages_only_declared_files() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        let info = preset_info();
        fs::write(
            source.join("plugin.yaml"),
            serde_yaml_ng::to_string(&info).unwrap(),
        )
        .unwrap();
        fs::write(source.join("preset.yaml"), "packages: [git]\n").unwrap();
        fs::write(source.join("README.md"), "Documentation\n").unwrap();
        handle_plugin_validate(source.to_str().unwrap()).unwrap();
        let plugins = root.path().join("state").join("plugins");
        install_validated_plugin(&source, &plugins, "presets", &info, "preset.yaml").unwrap();
        let target = plugins.join("presets").join(&info.name);
        assert!(target.join("plugin.yaml").is_file());
        assert!(target.join("preset.yaml").is_file());
        assert!(target.join("README.md").is_file());
        assert!(
            install_validated_plugin(&source, &plugins, "presets", &info, "preset.yaml").is_err()
        );
        fs::write(source.join("run.sh"), "echo hi\n").unwrap();
        assert!(validate_plugin_files(&source, "preset.yaml").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn plugin_install_rejects_links_and_keeps_target_absent() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("plugin.yaml"),
            serde_yaml_ng::to_string(&preset_info()).unwrap(),
        )
        .unwrap();
        fs::write(source.join("preset.yaml"), "packages: [git]\n").unwrap();
        symlink(source.join("preset.yaml"), source.join("linked.yaml")).unwrap();
        assert!(validate_plugin_files(&source, "preset.yaml").is_err());
        assert!(validate_plugin_source(&source.join("linked.yaml")).is_err());
        assert!(!root
            .path()
            .join("state/plugins/presets/unique-test-preset-plugin")
            .exists());
    }
}
