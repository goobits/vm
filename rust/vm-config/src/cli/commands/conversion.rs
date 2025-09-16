use anyhow::{Context, Result};
use std::path::PathBuf;

use super::super::{utils::output_config, OutputFormat};
use crate::config::VmConfig;

pub fn execute(input: PathBuf, format: OutputFormat) -> Result<()> {
    let config = VmConfig::from_file(&input)
        .with_context(|| format!("Failed to load config: {:?}", input))?;
    output_config(&config, &format)
}
