use std::path::PathBuf;
use vm_core::error::Result;

use super::validation::load_and_merge_config;
use crate::cli::formatting::output_shell_exports_from_config;
use serde_yaml_ng as serde_yaml;

pub fn execute_dump(file: Option<PathBuf>) -> Result<()> {
    let merged = load_and_merge_config(file)?;
    let yaml = serde_yaml::to_string(&merged)?;
    print!("{}", yaml);
    Ok(())
}

pub fn execute_export(file: Option<PathBuf>) -> Result<()> {
    let merged = load_and_merge_config(file)?;
    output_shell_exports_from_config(&merged);
    Ok(())
}
