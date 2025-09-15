use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vm_config::config::VmConfig;
use vm_provider::get_provider;

#[derive(Parser)]
#[command(name = "vm")]
#[command(about = "A modern, fast, and portable VM management tool")]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Path to a custom vm.yaml configuration file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Disable automatic preset detection
    #[arg(long, global = true)]
    no_preset: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a new vm.yaml configuration file
    Init {
        /// Custom configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Create and provision a new VM
    Create,
    /// Start an existing VM
    Start,
    /// Stop a running VM
    Stop,
    /// Destroy a VM and its resources
    Destroy,
    /// SSH into a VM
    Ssh {
        /// Optional path to start the shell in
        #[arg()]
        path: Option<PathBuf>,
    },
    /// Get the status of a VM
    Status,
    /// Execute a command in the VM
    Exec {
        /// The command to execute
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// View logs of the VM
    Logs,
    /// Validate the configuration
    Validate,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // For commands that don't need a provider, handle them first.
    match &args.command {
        Command::Validate => {
            // The `load` function performs validation internally. If it succeeds,
            // the configuration is valid.
            match VmConfig::load(args.config, args.no_preset) {
                Ok(_) => {
                    println!("✅ Configuration is valid.");
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("❌ Configuration is invalid: {:#}", e);
                    // Return the error to exit with a non-zero status code
                    return Err(e);
                }
            }
        }
        Command::Init { file } => {
            // Initialize a new vm.yaml configuration file
            return vm_config::cli::init_config_file(file.clone());
        }
        _ => {} // Continue to provider-based commands
    }

    // 1. Load configuration
    // The vm-config crate now handles file discovery, preset merging, and validation.
    let config = VmConfig::load(args.config, args.no_preset)?;

    // 2. Get the appropriate provider
    let provider = get_provider(config)?;

    // 3. Execute the command
    match args.command {
        Command::Create => provider.create(),
        Command::Start => provider.start(),
        Command::Stop => provider.stop(),
        Command::Destroy => provider.destroy(),
        Command::Ssh { path } => {
            let relative_path = path.unwrap_or_else(|| PathBuf::from("."));
            provider.ssh(&relative_path)
        }
        Command::Status => provider.status(),
        Command::Exec { command } => provider.exec(&command),
        Command::Logs => provider.logs(),
        Command::Validate => unreachable!(), // Handled above
        Command::Init { .. } => unreachable!(), // Handled above
    }
}
