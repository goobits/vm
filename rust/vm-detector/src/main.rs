// Standard library
use std::env;

// External crates
use anyhow::{Context, Result};

// Internal imports
use vm_detector::{detect_project_type, format_detected_types};

fn main() -> Result<()> {
    let project_dir = env::current_dir()
        .with_context(|| "Failed to get current directory for project detection")?;
    let detected_types = detect_project_type(&project_dir);
    let formatted = format_detected_types(detected_types);
    println!("{}", formatted);
    Ok(())
}
