// Standard library
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

// External crates
use anyhow::{Context, Result};
use colored::*;
use dialoguer::Confirm;

pub fn ensure_path(bin_dir: &Path) -> Result<()> {
    let path_var = env::var("PATH").unwrap_or_default();
    let is_in_path = path_var.split(':').any(|p| Path::new(p) == bin_dir);

    if is_in_path {
        println!("✅ {} is already in your PATH.", bin_dir.display());
        return Ok(());
    }

    println!("⚠️ {} is not in your PATH", bin_dir.display());

    let shell_profile = get_shell_profile()?;
    let Some(profile_path) = shell_profile else {
        println!(
            "Could not detect shell profile. Please add {} to your PATH manually.",
            bin_dir.display()
        );
        return Ok(());
    };
    let prompt = format!(
        "Add {} to your PATH in {:?}?",
        bin_dir.display(),
        profile_path
    );

    if Confirm::new().with_prompt(prompt).interact()? {
        add_to_profile(&profile_path, bin_dir)?;
        println!(
            "✅ Added PATH to {}. Please restart your shell.",
            profile_path.display()
        );
    } else {
        println!(
            "Please add the following to your shell profile:\n  {}",
            format!("export PATH=\"{}:$PATH\"", bin_dir.display()).cyan()
        );
    }

    Ok(())
}

fn get_shell_profile() -> Result<Option<PathBuf>> {
    let shell = env::var("SHELL").unwrap_or_default();
    let home = dirs::home_dir().context("Could not find home directory")?;

    Ok(match shell.split('/').next_back() {
        Some("bash") => Some(home.join(".bashrc")),
        Some("zsh") => Some(home.join(".zshrc")),
        Some("fish") => Some(home.join(".config/fish/config.fish")),
        _ => None,
    })
}

fn add_to_profile(profile_path: &Path, bin_dir: &Path) -> Result<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(profile_path)?;

    let line_to_add = if profile_path.ends_with("config.fish") {
        format!("\nfish_add_path -p \"{}\"", bin_dir.display())
    } else {
        format!(
            "\n# Added by VM tool installer\nexport PATH=\"{}:$PATH\"",
            bin_dir.display()
        )
    };

    writeln!(file, "{}", line_to_add).context("Failed to write to shell profile")
}

/// Detect platform string for use in build target directories
pub fn detect_platform_string() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    format!("{}-{}", os, arch)
}
