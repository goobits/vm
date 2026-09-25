use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use vm_core::{msg, vm_println};
use vm_messages::messages::MESSAGES;
use vm_plugin::{is_valid_plugin_name, PluginType};

use super::validation::{plugin_from_source, validate_all, validate_plugin_files};

pub(super) fn handle_plugin_install(source_path: &str) -> Result<()> {
    let source = PathBuf::from(source_path);
    let temp_plugin = plugin_from_source(&source)?;
    let info = &temp_plugin.info;
    let content_file = match info.plugin_type {
        PluginType::Preset => "preset.yaml",
        PluginType::Service => "service.yaml",
    };

    // Validate plugin before installation
    vm_println!("{}", MESSAGES.plugin.install_validating);
    let validation_result = validate_all(&temp_plugin)?;

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

pub(super) fn handle_plugin_remove(plugin_name: &str) -> Result<()> {
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

pub(super) fn install_validated_plugin(
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
    let staged_validation = validate_all(&staged_plugin)?;
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
