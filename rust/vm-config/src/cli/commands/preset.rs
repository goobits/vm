use anyhow::Result;
use std::path::PathBuf;

use super::super::{utils::output_config, OutputFormat};
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
                eprintln!("No preset detected for project");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
