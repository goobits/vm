// Configuration-related command handlers

use anyhow::{Context, Result};
use log::debug;
use std::path::PathBuf;

use crate::cli::ConfigSubcommand;
use vm_common::{vm_error, vm_success};
use vm_config::{config::VmConfig, ConfigOps};
use serde_yaml_ng as serde_yaml;

/// Handle configuration validation command
pub fn handle_validate(config_file: Option<PathBuf>, no_preset: bool) -> Result<()> {
    debug!(
        "Validating configuration: config_file={:?}, no_preset={}",
        config_file, no_preset
    );
    // The `load` function performs validation internally. If it succeeds,
    // the configuration is valid.
    match VmConfig::load(config_file, no_preset) {
        Ok(config) => {
            debug!(
                "Configuration validation successful: provider={:?}, project_name={:?}",
                config.provider,
                config.project.as_ref().and_then(|p| p.name.as_ref())
            );
            vm_success!("Configuration is valid.");
            Ok(())
        }
        Err(e) => {
            debug!("Configuration validation failed: {}", e);
            vm_error!("Configuration is invalid: {:#}", e);
            // Return the error to exit with a non-zero status code
            Err(e)
        }
    }
}

/// Handle configuration management commands
pub fn handle_config_command(command: &ConfigSubcommand) -> Result<()> {
    match command {
        ConfigSubcommand::Set {
            field,
            value,
            global,
        } => ConfigOps::set(field, value, *global),
        ConfigSubcommand::Get { field, global } => ConfigOps::get(field.as_deref(), *global),
        ConfigSubcommand::Unset { field, global } => ConfigOps::unset(field, *global),
        ConfigSubcommand::Clear { global } => ConfigOps::clear(*global),
        ConfigSubcommand::Preset {
            names,
            global,
            list,
            show,
        } => match (list, show, names) {
            (true, _, _) => ConfigOps::preset("", *global, true, None),
            (_, Some(show_name), _) => ConfigOps::preset("", *global, false, Some(show_name)),
            (_, _, Some(preset_names)) => ConfigOps::preset(preset_names, *global, false, None),
            _ => Ok(()),
        },
    }
}

/// Load configuration with lenient validation for commands that don't require full project setup
pub fn load_config_lenient(file: Option<PathBuf>, _no_preset: bool) -> Result<VmConfig> {
    use vm_config::config::VmConfig;

    // Try to load defaults as base
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../../defaults.yaml");
    let mut config: VmConfig =
        serde_yaml::from_str(EMBEDDED_DEFAULTS).context("Failed to parse embedded defaults")?;

    // Try to find and load user config if it exists
    let user_config_path = match file {
        Some(path) => Some(path),
        None => {
            // Look for vm.yaml in current directory
            let current_dir = std::env::current_dir()?;
            let vm_yaml_path = current_dir.join("vm.yaml");
            if vm_yaml_path.exists() {
                Some(vm_yaml_path)
            } else {
                None
            }
        }
    };

    if let Some(path) = user_config_path {
        match VmConfig::from_file(&path) {
            Ok(user_config) => {
                // Merge user config into defaults using available public API
                // For lenient loading, we'll do a simple field-by-field merge
                if user_config.provider.is_some() {
                    config.provider = user_config.provider;
                }
                if user_config.project.is_some() {
                    config.project = user_config.project;
                }
                if user_config.vm.is_some() {
                    config.vm = user_config.vm;
                }
                // Copy other important fields
                if !user_config.services.is_empty() {
                    config.services = user_config.services;
                }
            }
            Err(e) => {
                debug!("Failed to load user config, using defaults: {}", e);
            }
        }
    }

    // Ensure we have at least a minimal valid config for providers
    if config.provider.is_none() {
        config.provider = Some(String::from("docker"));
    }

    Ok(config)
}
