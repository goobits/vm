// External crates
use anyhow::Result;
use clap::{Parser, Subcommand};

// Internal imports
use crate::installer::PackageInstaller;
use crate::package_manager::PackageManager;

#[derive(Parser)]
#[command(name = "vm-pkg")]
#[command(about = "Unified package manager for VM Tool")]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum LinksSubcommand {
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

#[derive(Subcommand)]
pub enum Command {
    /// Install a package
    Install {
        /// Package manager type
        #[arg(short = 't', long, value_enum)]
        package_type: PackageManager,

        /// Package name to install
        package: String,

        /// User to install for
        #[arg(short = 'u', long, default_value = "developer")]
        user: String,

        /// Force registry installation (ignore linked packages)
        #[arg(short = 'f', long)]
        force_registry: bool,
    },

    /// Check if a package is linked
    Check {
        /// Package manager type
        #[arg(short = 't', long, value_enum)]
        package_type: PackageManager,

        /// Package name to check
        package: String,

        /// User to check for
        #[arg(short = 'u', long, default_value = "developer")]
        user: String,
    },

    /// List linked packages
    List {
        /// Package manager type (optional, lists all if not specified)
        #[arg(short = 't', long, value_enum)]
        package_type: Option<PackageManager>,

        /// User to list for
        #[arg(short = 'u', long, default_value = "developer")]
        user: String,
    },

    /// System-wide package link detection and mount generation
    Links {
        #[command(subcommand)]
        command: LinksSubcommand,
    },
}

pub fn execute(args: Args) -> Result<()> {
    match args.command {
        Command::Install {
            package_type,
            package,
            user,
            force_registry,
        } => {
            let installer = PackageInstaller::new(user);
            installer.install(&package, package_type, force_registry)?;
        }
        Command::Check {
            package_type,
            package,
            user,
        } => {
            let installer = PackageInstaller::new(user);
            if installer.is_linked(&package, package_type)? {
                println!("🔗 Package '{}' is linked for {}", package, package_type);
            } else {
                println!(
                    "📦 Package '{}' is not linked (would install from registry)",
                    package
                );
            }
        }
        Command::List { package_type, user } => {
            let installer = PackageInstaller::new(user);
            installer.list_linked(package_type);
        }
        Command::Links { command } => {
            use crate::links::{detect_packages, validate_package_manager};
            match command {
                LinksSubcommand::Detect {
                    package_manager,
                    packages,
                } => {
                    validate_package_manager(&package_manager)?;
                    let detections = detect_packages(&package_manager, &packages)?;

                    for (package, path) in detections {
                        println!("{}:{}", package, path);
                    }
                }
                LinksSubcommand::Mounts {
                    package_manager,
                    packages,
                } => {
                    validate_package_manager(&package_manager)?;
                    let detections = detect_packages(&package_manager, &packages)?;

                    for (package, path) in detections {
                        println!(
                            "{}:/home/developer/.links/{}/{}:delegated",
                            path, package_manager, package
                        );
                        eprintln!(
                            "📦 Found linked package ({}): {} -> {}",
                            package_manager, package, path
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
