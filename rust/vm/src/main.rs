use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_yaml;
use std::path::PathBuf;
use vm_config::config::VmConfig;
use vm_config::preset::PresetDetector;
use vm_config::paths;
use vm_provider::get_provider;
use vm_provider::progress::{confirm_prompt, ProgressReporter, StatusFormatter};

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
enum PresetSubcommand {
    /// List available presets
    List,
    /// Show details of a specific preset
    Show {
        /// Name of the preset to show
        name: String,
    },
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
    /// Restart a VM (stop then start)
    Restart,
    /// Re-run provisioning on existing VM
    Provision,
    /// List all VMs
    List,
    /// Force kill VM processes
    Kill,
    /// Get workspace directory
    GetSyncDirectory,
    /// Preset operations
    Preset {
        #[command(subcommand)]
        command: PresetSubcommand,
    },
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
        Command::Preset { command } => {
            // Handle preset commands
            let project_dir = std::env::current_dir()?;
            let presets_dir = paths::get_presets_dir();
            let detector = PresetDetector::new(project_dir, presets_dir);

            match command {
                PresetSubcommand::List => {
                    println!("Available presets:");
                    let presets = detector.list_presets()?;
                    for preset in presets {
                        println!("  {}", preset);
                    }
                    return Ok(());
                }
                PresetSubcommand::Show { name } => {
                    let config = detector.load_preset(name)?;
                    let yaml = serde_yaml::to_string(&config)?;
                    println!("Preset '{}' configuration:", name);
                    println!("{}", yaml);
                    return Ok(());
                }
            }
        }
        _ => {} // Continue to provider-based commands
    }

    // 1. Load configuration
    // The vm-config crate now handles file discovery, preset merging, and validation.
    let config = VmConfig::load(args.config, args.no_preset)?;

    // 2. Get the appropriate provider
    let provider = get_provider(config.clone())?;

    // 3. Execute the command
    match args.command {
        Command::Create => provider.create(),
        Command::Start => provider.start(),
        Command::Stop => provider.stop(),
        Command::Restart => provider.restart(),
        Command::Provision => provider.provision(),
        Command::List => provider.list(),
        Command::Kill => provider.kill(),
        Command::GetSyncDirectory => {
            let sync_dir = provider.get_sync_directory()?;
            println!("{}", sync_dir);
            Ok(())
        },
        Command::Destroy => {
            // Get VM name from config for confirmation prompt
            let vm_name = config.project
                .as_ref()
                .and_then(|p| p.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("VM");

            // Initialize progress reporter
            let progress = ProgressReporter::new();

            // Show confirmation prompt
            progress.phase_header("🗑️", "DESTROY PHASE");
            let confirmation_msg = format!("├─ ⚠️  Are you sure you want to destroy {}? This will delete all data. (y/N): ", vm_name);

            if confirm_prompt(&confirmation_msg) {
                progress.subtask("├─", "Proceeding with destruction...");
                let result = provider.destroy();
                match result {
                    Ok(()) => progress.complete("└─", "VM destroyed successfully"),
                    Err(e) => {
                        progress.error("└─", &format!("Destruction failed: {}", e));
                        return Err(e);
                    }
                }
                result
            } else {
                progress.error("└─", "Destruction cancelled");
                std::process::exit(1);
            }
        }
        Command::Ssh { path } => {
            let relative_path = path.unwrap_or_else(|| PathBuf::from("."));
            provider.ssh(&relative_path)
        }
        Command::Status => {
            // Enhanced status reporting using StatusFormatter
            let progress = ProgressReporter::new();
            let status_formatter = StatusFormatter::new();

            progress.phase_header("📊", "STATUS CHECK");
            progress.subtask("├─", "Checking VM status...");

            // Get VM name from config
            let vm_name = config.project.as_ref()
                .and_then(|p| p.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("vm-project");

            // Get memory and cpu info from config
            let memory = config.vm.as_ref().and_then(|vm| vm.memory);
            let cpus = config.vm.as_ref().and_then(|vm| vm.cpus);

            let result = provider.status();
            match result {
                Ok(()) => {
                    progress.complete("└─", "Status check complete");

                    // Format status information
                    println!("\n");
                    status_formatter.format_status(
                        vm_name,
                        "running", // This could be enhanced to get actual status
                        provider.name(),
                        memory,
                        cpus
                    );
                }
                Err(e) => {
                    progress.error("└─", &format!("Status check failed: {}", e));
                    return Err(e);
                }
            }
            result
        }
        Command::Exec { command } => provider.exec(&command),
        Command::Logs => provider.logs(),
        Command::Validate => unreachable!(), // Handled above
        Command::Init { .. } => unreachable!(), // Handled above
        Command::Preset { .. } => unreachable!(), // Handled above
    }
}
