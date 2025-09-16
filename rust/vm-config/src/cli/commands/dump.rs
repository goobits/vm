use anyhow::Result;
use std::path::PathBuf;

use super::super::utils::output_shell_exports;
use super::validation::load_and_merge_config;

pub fn execute_dump(file: Option<PathBuf>, no_preset: bool) -> Result<()> {
    let merged = load_and_merge_config(file, no_preset)?;
    let yaml = serde_yaml::to_string(&merged)?;
    print!("{}", yaml);
    Ok(())
}

pub fn execute_export(file: Option<PathBuf>, no_preset: bool) -> Result<()> {
    let merged = load_and_merge_config(file, no_preset)?;
    let value = serde_yaml::to_value(&merged)?;
    output_shell_exports(&value)
}