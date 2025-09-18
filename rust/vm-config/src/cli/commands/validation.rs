// Standard library
use std::path::{Path, PathBuf};

// External crates
use anyhow::{Context, Result};
use vm_common::{vm_error, vm_println, vm_success};

// Internal imports
use crate::cli::formatting::find_vm_config_file;
use crate::{config::VmConfig, paths};

pub fn execute_validate(file: Option<PathBuf>, verbose: bool) {
    match load_and_merge_config(file) {
        Ok(_) => {
            vm_success!("Configuration is valid");
            if verbose {
                vm_println!("Successfully loaded, merged, and validated the configuration.");
            }
        }
        Err(e) => {
            vm_error!("Configuration validation failed: {:#}", e);
            std::process::exit(1);
        }
    }
}

#[must_use = "file validation results should be handled"]
pub fn execute_check_file(file: PathBuf) -> Result<()> {
    use crate::yaml::YamlOperations;
    match YamlOperations::validate_file(&file) {
        Ok(_) => {
            vm_success!("File is valid YAML");
            std::process::exit(0);
        }
        Err(e) => {
            vm_error!("File validation failed: {}", e);
            std::process::exit(1);
        }
    }
}

#[must_use = "configuration loading results should be used"]
pub fn load_and_merge_config(file: Option<PathBuf>) -> Result<VmConfig> {
    // 1. Find user config file, if any
    let user_config_path = match file {
        Some(path) => Some(path),
        None => find_vm_config_file().ok(), // It's okay if it's not found
    };

    // 2. Load user config if path was found
    let user_config = if let Some(ref path) = user_config_path {
        Some(
            VmConfig::from_file(path)
                .with_context(|| format!("Failed to load user config from: {:?}", path))?,
        )
    } else {
        None
    };

    // 3. Determine project directory for preset detection
    let project_dir = match user_config_path {
        Some(ref path) => path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
        None => std::env::current_dir()?,
    };

    // 4. Load OS-specific defaults as the new base
    let default_os = crate::detector::detect_host_os();
    let detected_os = user_config
        .as_ref()
        .and_then(|c| c.os.as_deref())
        .unwrap_or(&default_os);

    let os_defaults_path = paths::get_config_dir()
        .join("os_defaults")
        .join(format!("{}.yaml", detected_os));
    let os_defaults_config = if os_defaults_path.exists() {
        VmConfig::from_file(&os_defaults_path)?
    } else {
        VmConfig::default()
    };

    // 5. Detect and load project-specific preset (only if no user config exists)
    let presets_dir = crate::paths::get_presets_dir();
    let preset_config = if user_config.is_none() {
        let detector = crate::preset::PresetDetector::new(project_dir, presets_dir);
        if let Some(preset_name) = detector.detect() {
            if preset_name != "base" && preset_name != "generic" {
                Some(detector.load_preset(&preset_name)?)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        // User has a config file, don't override with auto-detected presets
        None
    };

    // 6. Load global configuration if it exists
    let global_config = crate::config_ops::load_global_config();

    // 7. Merge in order: os_defaults -> global -> preset -> user
    let merged = crate::merge::merge_configs(
        Some(os_defaults_config),
        global_config,
        preset_config,
        user_config,
    )?;

    // 8. Validate the final merged configuration against the schema
    let schema_path = crate::paths::get_schema_path();
    let validator = crate::validate::ConfigValidator::new(merged.clone(), schema_path);
    validator
        .validate()
        .with_context(|| "Final configuration validation failed")?;

    Ok(merged)
}
