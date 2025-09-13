use clap::{Parser, Subcommand};
use anyhow::Result;

mod npm;
mod pip;
mod cargo;

#[derive(Parser)]
#[command(name = "vm-links")]
#[command(about = "Package link detection for VM Tool")]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Detect linked packages and output package:path pairs
    Detect {
        /// Package manager (npm, pip, cargo)
        package_manager: String,
        /// Package names to detect
        packages: Vec<String>,
    },
    /// Generate Docker mount strings for linked packages
    Mounts {
        /// Package manager (npm, pip, cargo)
        package_manager: String,
        /// Package names to detect
        packages: Vec<String>,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Detect { package_manager, packages } => {
            validate_package_manager(&package_manager)?;
            let detections = detect_packages(&package_manager, &packages)?;

            for (package, path) in detections {
                println!("{}:{}", package, path);
            }
        }
        Command::Mounts { package_manager, packages } => {
            validate_package_manager(&package_manager)?;
            let detections = detect_packages(&package_manager, &packages)?;

            for (package, path) in detections {
                println!("{}:/home/developer/.links/{}/{}:delegated", path, package_manager, package);
                eprintln!("📦 Found linked package ({}): {} -> {}", package_manager, package, path);
            }
        }
    }

    Ok(())
}

fn validate_package_manager(pm: &str) -> Result<()> {
    match pm {
        "npm" | "pip" | "cargo" => Ok(()),
        _ => anyhow::bail!("❌ Error: Package manager '{}' not in whitelist: [npm, pip, cargo]", pm),
    }
}

fn detect_packages(package_manager: &str, packages: &[String]) -> Result<Vec<(String, String)>> {
    match package_manager {
        "npm" => npm::detect_npm_packages(packages),
        "pip" => pip::detect_pip_packages(packages),
        "cargo" => cargo::detect_cargo_packages(packages),
        _ => unreachable!(), // Already validated
    }
}