mod range;
mod registry;

// External crates
use anyhow::Result;
use clap::{Parser, Subcommand};

// Internal imports
use range::PortRange;
use registry::PortRegistry;

#[derive(Parser)]
#[command(name = "vm-ports")]
#[command(about = "Port range management for VM Tool")]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Check for port range conflicts
    Check {
        /// Port range (e.g., "3000-3009")
        range: String,
        /// Optional project name to exclude from conflict checking
        project_name: Option<String>,
    },
    /// Register a port range for a project
    Register {
        /// Port range (e.g., "3000-3009")
        range: String,
        /// Project name
        project: String,
        /// Project path
        path: String,
    },
    /// Suggest next available port range
    Suggest {
        /// Range size (default: 10)
        size: Option<u16>,
    },
    /// List all registered port ranges
    List,
    /// Unregister a project's port range
    Unregister {
        /// Project name
        project: String,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Check {
            range,
            project_name,
        } => {
            let port_range = PortRange::parse(&range)?;
            let registry = PortRegistry::load()?;

            if let Some(conflicts) = registry.check_conflicts(&port_range, project_name.as_deref())
            {
                println!("{}", conflicts);
                std::process::exit(1);
            } else {
                std::process::exit(0);
            }
        }
        Command::Register {
            range,
            project,
            path,
        } => {
            let port_range = PortRange::parse(&range)?;
            let mut registry = PortRegistry::load()?;

            if let Some(conflicts) = registry.check_conflicts(&port_range, Some(&project)) {
                println!("⚠️  Port range {} conflicts with: {}", range, conflicts);
                std::process::exit(1);
            } else {
                registry.register(&project, &port_range, &path)?;
                println!(
                    "✅ Registered port range {} for project '{}'",
                    range, project
                );
            }
        }
        Command::Suggest { size } => {
            let registry = PortRegistry::load()?;
            let size = size.unwrap_or(10);

            if let Some(range) = registry.suggest_next_range(size, 3000) {
                println!("{}", range);
            } else {
                eprintln!("❌ No available port range of size {} found", size);
                std::process::exit(1);
            }
        }
        Command::List => {
            let registry = PortRegistry::load()?;
            registry.list();
        }
        Command::Unregister { project } => {
            let mut registry = PortRegistry::load()?;
            registry.unregister(&project)?;
            println!("✅ Unregistered port range for project '{}'", project);
        }
    }

    Ok(())
}
