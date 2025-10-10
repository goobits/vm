// Standard library
use std::path::PathBuf;

// External crates
use tracing::{error, info};
use vm_core::error::Result;

// Internal imports
use crate::{config::VmConfig, loader::ConfigLoader};

pub fn execute_validate(file: Option<PathBuf>, verbose: bool) {
    match load_and_merge_config(file) {
        Ok(_) => {
            info!("✅ Configuration is valid");
            if verbose {
                info!("Successfully loaded the configuration.");
            }
        }
        Err(e) => {
            error!("❌ Configuration validation failed: {:#}", e);
            std::process::exit(1);
        }
    }
}

#[must_use = "file validation results should be handled"]
pub fn execute_check_file(file: PathBuf) -> Result<()> {
    use crate::yaml::YamlOperations;
    match YamlOperations::validate_file(&file) {
        Ok(_) => {
            info!("✅ File is valid YAML");
            std::process::exit(0);
        }
        Err(e) => {
            error!("❌ File validation failed: {}", e);
            std::process::exit(1);
        }
    }
}

use vm_core::error::VmError;

#[must_use = "configuration loading results should be used"]
pub fn load_and_merge_config(_file: Option<PathBuf>) -> Result<VmConfig> {
    // The `file` argument is now ignored, as the new loader handles discovery.
    // It's kept for API compatibility for now.
    ConfigLoader::new()
        .load()
        .map_err(|e| VmError::Config(e.to_string()))
}
