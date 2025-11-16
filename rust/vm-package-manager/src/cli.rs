// External crates
use clap::{Parser, Subcommand};
use vm_cli::msg;
use vm_core::error::Result;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;

// Internal imports
use crate::installer::PackageInstaller;
use crate::package_manager::PackageManager;

/// Command-line arguments for the VM package manager.
///
/// This structure defines the top-level arguments and subcommands available
/// for the vm-package-manager tool, which provides unified package management across
/// different package managers (npm, pip, cargo, etc.) within VM environments.
#[derive(Parser)]
#[command(name = "vm-package-manager")]
#[command(about = "Unified package manager for VM Tool")]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands for managing linked packages.
///
/// These commands help detect and manage locally linked packages across different
/// package managers, enabling development workflows where packages are linked
/// from local directories rather than installed from registries.
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
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.service.pkg_manager_linked,
                        package = &package,
                        r#type = format!("{:?}", package_type)
                    )
                );
            } else {
                vm_println!(
                    "{}",
                    msg!(MESSAGES.service.pkg_manager_not_linked, package = &package)
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
                        println!("{package}:{path}");
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
                            "{path}:/home/developer/.links/{package_manager}/{package}:delegated"
                        );
                        eprintln!(
                            "📦 Found linked package ({package_manager}): {package} -> {path}"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
