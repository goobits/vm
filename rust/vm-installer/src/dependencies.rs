use anyhow::{bail, Result};
use colored::*;
use which::which;

pub fn check() -> Result<()> {
    println!("🔍 Checking dependencies...");
    if which("cargo").is_err() || which("rustc").is_err() {
        bail!(
            "{} is not installed or not in your PATH.\n{}",
            "Rust/Cargo".yellow(),
            "Please install the Rust toolchain from https://rustup.rs to continue.".cyan()
        );
    }
    println!("{}", "✅ Dependencies satisfied".green());
    Ok(())
}

