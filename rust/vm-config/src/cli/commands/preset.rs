//! Preset detection and management commands.
//!
//! This module provides functionality for detecting, listing, and loading VM configuration
//! presets based on project structure and technology stack. Presets automatically configure
//! VM environments for common development frameworks and languages.
//!
//! ## Commands
//!
//! - **Detection**: Automatically detect appropriate preset for current project
//! - **Listing**: Show all available presets in the presets directory
//! - **Loading**: Load and display preset configuration in YAML format
//!
//! ## Usage Examples
//!
//! ```bash
//! # Detect preset for current project
//! vm-config preset --detect-only
//!
//! # List all available presets
//! vm-config preset --list
//!
//! # Load and display detected preset configuration
//! vm-config preset
//! ```
//!
//! The preset detector examines project files (package.json, Cargo.toml, etc.) to
//! automatically determine the most appropriate VM configuration preset.

use anyhow::Result;
use std::path::PathBuf;
use vm_common::vm_error;

use crate::cli::formatting::output_config;
use crate::cli::OutputFormat;
use crate::{paths, preset::PresetDetector};

pub fn execute(
    dir: PathBuf,
    presets_dir: Option<PathBuf>,
    detect_only: bool,
    list: bool,
) -> Result<()> {
    let presets_dir = presets_dir.unwrap_or_else(paths::get_presets_dir);
    let detector = PresetDetector::new(dir, presets_dir);

    if list {
        let presets = detector.list_presets()?;
        println!("Available presets:");
        for preset in presets {
            println!("  - {}", preset);
        }
    } else if detect_only {
        match detector.detect() {
            Some(preset) => println!("{}", preset),
            None => println!("base"),
        }
    } else {
        match detector.detect() {
            Some(preset_name) => {
                let preset = detector.load_preset(&preset_name)?;
                output_config(&preset, &OutputFormat::Yaml)?;
            }
            None => {
                vm_error!("No preset detected for project");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
