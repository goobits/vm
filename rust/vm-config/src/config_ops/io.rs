// Standard library
use std::fs;
use std::path::{Path, PathBuf};

// External crates

// Internal imports
use crate::config::VmConfig;
use vm_core::error::{Result, VmError};
use vm_core::{prompts, user_paths, vm_println};

/// Read config file, or prompt to initialize if missing (for write operations).
pub fn read_config_or_init(path: &Path, allow_init: bool) -> Result<VmConfig> {
    if path.exists() {
        return VmConfig::from_file(&path.to_path_buf());
    }

    if !allow_init {
        return Err(VmError::Config(format!(
            "No configuration found: {}",
            path.display()
        )));
    }

    use std::io::IsTerminal;
    let is_interactive = std::io::stdin().is_terminal();

    if is_interactive {
        vm_println!("⚠️  No configuration found: {}", path.display());
        vm_println!();
        if !prompts::confirm_select("Initialize new project?", true)
            .map_err(|e| VmError::Config(format!("Failed to read selection: {e}")))?
        {
            return Err(VmError::Config(
                "Configuration required. Create an environment with `vm run linux` first."
                    .to_string(),
            ));
        }

        vm_println!();
        vm_println!("✨ Initializing project...");
    }

    super::init_config_file(Some(path.to_path_buf()), None, None, None)?;
    vm_println!();
    VmConfig::from_file(&path.to_path_buf())
}

pub fn get_global_config_path() -> PathBuf {
    user_paths::global_config_path().unwrap_or_else(|_| {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".vm").join("config.yaml")
        } else {
            PathBuf::from(".vm/config.yaml")
        }
    })
}

pub fn get_or_create_global_config_path() -> Result<PathBuf> {
    let config_path = get_global_config_path();
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                VmError::Filesystem(format!(
                    "Failed to create config directory: {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
    }
    Ok(config_path)
}

pub fn find_local_config() -> Result<PathBuf> {
    find_local_config_impl()
}

fn find_local_config_impl() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    let mut dir = current_dir.as_path();

    loop {
        let config_path = dir.join("vm.yaml");
        if config_path.exists() {
            return Ok(config_path);
        }

        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    Err(vm_core::error::VmError::Config(
        "No vm.yaml configuration found in the current directory or its parents. Create an environment with `vm run linux` first".to_string()
    ))
}

pub fn find_or_create_local_config() -> Result<PathBuf> {
    if let Ok(path) = find_local_config_impl() {
        return Ok(path);
    }
    let current_dir = std::env::current_dir()?;
    Ok(current_dir.join("vm.yaml"))
}

/// Load global configuration if it exists
pub fn load_global_config() -> Option<VmConfig> {
    let global_path = get_global_config_path();
    if !global_path.exists() {
        return None;
    }
    VmConfig::from_file(&global_path).ok()
}
