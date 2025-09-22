//! Configuration format conversion commands.
//!
//! This module provides functionality for converting VM configuration files between
//! different output formats (YAML and JSON). It loads configuration files and outputs
//! them in the specified format while preserving all configuration data and structure.
//!
//! ## Commands
//!
//! - **convert**: Load a configuration file and output it in the specified format
//!
//! ## Supported Formats
//!
//! - **YAML**: Human-readable format (default)
//! - **JSON**: Machine-readable format for integration
//!
//! ## Usage Examples
//!
//! ```bash
//! # Convert configuration to JSON format
//! vm-config convert --input vm.yaml --format json
//!
//! # Convert configuration to YAML format (explicit)
//! vm-config convert --input config.json --format yaml
//! ```
//!
//! The conversion process validates the input configuration and ensures
//! proper formatting in the target output format.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::cli::formatting::output_config;
use crate::cli::OutputFormat;
use crate::config::VmConfig;

pub fn execute(input: PathBuf, format: OutputFormat) -> Result<()> {
    let config = VmConfig::from_file(&input)
        .with_context(|| format!("Failed to load config: {:?}", input))?;
    output_config(&config, &format)
}
