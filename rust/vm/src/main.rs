use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicBool, Ordering};
use vm_config::config::VmConfig;
use vm_provider::get_provider;
use vm_provider::progress::{confirm_prompt, ProgressReporter, StatusFormatter};

// Global debug flag (thread-safe)
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

// Debug output macro
macro_rules! debug {
    ($($arg:tt)*) => {
        if DEBUG_ENABLED.load(Ordering::Relaxed) {
            eprintln!("DEBUG: {}", format!($($arg)*));
        }
    };
}

#[derive(Debug, Parser)]
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

    /// Show what would be executed without running
    #[arg(long, global = true)]
    dry_run: bool,

    /// Force specific preset
    #[arg(long, global = true)]
    preset: Option<String>,

    /// Enable debug output
    #[arg(short, long, global = true)]
    debug: bool,
}

#[derive(Debug, Subcommand)]
enum PresetSubcommand {
    /// List available presets
    List,
    /// Show details of a specific preset
    Show {
        /// Name of the preset to show
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigSubcommand {
    /// Set a configuration value
    Set {
        /// Field path (e.g., "vm.memory" or "services.docker.enabled")
        field: String,
        /// Value to set
        value: String,
        /// Apply to global config (~/.config/vm/global.yaml)
        #[arg(long)]
        global: bool,
    },
    /// Get configuration value(s)
    Get {
        /// Field path (omit to show all configuration)
        field: Option<String>,
        /// Read from global config
        #[arg(long)]
        global: bool,
    },
    /// Remove a configuration field
    Unset {
        /// Field path to remove
        field: String,
        /// Remove from global config
        #[arg(long)]
        global: bool,
    },
    /// Clear all configuration
    Clear {
        /// Clear global config
        #[arg(long)]
        global: bool,
    },
    /// Apply preset(s) to configuration
    Preset {
        /// Preset name(s), comma-separated for multiple (e.g., "nodejs,docker")
        names: Option<String>,
        /// Apply to global config
        #[arg(long)]
        global: bool,
        /// List available presets
        #[arg(long)]
        list: bool,
        /// Show preset details
        #[arg(long)]
        show: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum TempSubcommand {
    /// Create temp VM with mounts
    Create {
        /// Directories to mount (e.g., ./src,./config:ro)
        mounts: Vec<String>,

        /// Auto-destroy on exit
        #[arg(long)]
        auto_destroy: bool,
    },
    /// SSH into temp VM
    Ssh,
    /// Show temp VM status
    Status,
    /// Destroy temp VM
    Destroy,
    /// Add mount to running temp VM
    Mount {
        /// Path to mount (e.g., ./src or ./config:ro)
        path: String,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// Remove mount from temp VM
    Unmount {
        /// Path to unmount (omit for --all)
        path: Option<String>,
        /// Remove all mounts
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// List current mounts
    Mounts,
    /// List all temp VMs
    List,
    /// Stop temp VM
    Stop,
    /// Start temp VM
    Start,
    /// Restart temp VM
    Restart,
}

#[derive(Debug, Subcommand)]
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
    /// Generate configuration by composing services
    Generate {
        /// Comma-separated list of services (postgresql,redis,docker)
        #[arg(long)]
        services: Option<String>,

        /// Starting port for allocation (allocates 10 sequential ports)
        #[arg(long)]
        ports: Option<u16>,

        /// Project name (sets project.name, hostname, username)
        #[arg(long)]
        name: Option<String>,

        /// Output file (default: vm.yaml)
        output: Option<PathBuf>,
    },
    /// Manage temporary VMs
    Temp {
        #[command(subcommand)]
        command: TempSubcommand,
    },
    /// Alias for temp command
    Tmp {
        #[command(subcommand)]
        command: TempSubcommand,
    },
    /// Manage configuration settings
    Config {
        #[command(subcommand)]
        command: ConfigSubcommand,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Set global debug flag
    DEBUG_ENABLED.store(args.debug, Ordering::Relaxed);

    debug!("Starting vm command with args: {:?}", args);

    // For commands that don't need a provider, handle them first.
    match &args.command {
        Command::Validate => {
            debug!("Validating configuration: config_file={:?}, no_preset={}", args.config, args.no_preset);
            // The `load` function performs validation internally. If it succeeds,
            // the configuration is valid.
            match VmConfig::load(args.config, args.no_preset) {
                Ok(config) => {
                    debug!("Configuration validation successful: provider={:?}, project_name={:?}",
                           config.provider, config.project.as_ref().and_then(|p| p.name.as_ref()));
                    println!("✅ Configuration is valid.");
                    return Ok(());
                }
                Err(e) => {
                    debug!("Configuration validation failed: {}", e);
                    eprintln!("❌ Configuration is invalid: {:#}", e);
                    // Return the error to exit with a non-zero status code
                    return Err(e);
                }
            }
        }
        Command::Init { file } => {
            debug!("Delegating to vm-config binary: init command");
            return delegate_to_vm_config(&["init"], file.as_ref());
        }
        Command::Generate { services, ports, name, output } => {
            debug!("Delegating to vm-generator binary");
            return delegate_to_vm_generator(services.as_ref(), *ports, name.as_ref(), output.as_ref());
        }
        Command::Temp { command } | Command::Tmp { command } => {
            debug!("Delegating to vm-temp binary");
            return delegate_to_vm_temp(command);
        }
        Command::Config { command } => {
            debug!("Delegating to vm-config binary: config command");
            return delegate_to_vm_config_for_config(command);
        }
        Command::Preset { command } => {
            debug!("Delegating to vm-config binary: preset command");
            return delegate_to_vm_config_for_preset(command);
        }
        _ => {} // Continue to provider-based commands
    }

    // Handle dry-run for provider commands
    if args.dry_run {
        match &args.command {
            Command::Create | Command::Start | Command::Stop |
            Command::Restart | Command::Destroy | Command::Provision |
            Command::Kill => {
                println!("🔍 DRY RUN MODE - showing what would be executed:");
                println!("   Command: {:?}", args.command);
                if let Some(config) = &args.config {
                    println!("   Config: {}", config.display());
                }
                if let Some(preset) = &args.preset {
                    println!("   Preset override: {}", preset);
                }
                println!("🚫 Dry run complete - no commands were executed");
                return Ok(());
            }
            _ => {} // Non-provider commands proceed normally
        }
    }

    // 1. Load configuration
    // The vm-config crate now handles file discovery, preset merging, and validation.
    debug!("Loading configuration: config_file={:?}, no_preset={}, preset_override={:?}",
           args.config, args.no_preset, args.preset);

    let config = if let Some(preset) = args.preset {
        debug!("Using preset override: {}", preset);
        // TODO: Implement load_with_preset in vm-config
        // For now, use regular load
        VmConfig::load(args.config, args.no_preset)?
    } else {
        VmConfig::load(args.config, args.no_preset)?
    };

    debug!("Loaded configuration: provider={:?}, project_name={:?}",
           config.provider, config.project.as_ref().and_then(|p| p.name.as_ref()));

    // 2. Get the appropriate provider
    let provider = get_provider(config.clone())?;
    debug!("Using provider: {}", provider.name());

    // 3. Execute the command
    debug!("Executing command: {:?}", args.command);
    match args.command {
        Command::Create => {
            debug!("Starting VM creation");
            provider.create()
        },
        Command::Start => {
            debug!("Starting VM");
            provider.start()
        },
        Command::Stop => {
            debug!("Stopping VM");
            provider.stop()
        },
        Command::Restart => {
            debug!("Restarting VM");
            provider.restart()
        },
        Command::Provision => {
            debug!("Re-running VM provisioning");
            provider.provision()
        },
        Command::List => {
            debug!("Listing VMs");
            provider.list()
        },
        Command::Kill => {
            debug!("Force killing VM processes");
            provider.kill()
        },
        Command::GetSyncDirectory => {
            debug!("Getting sync directory for provider '{}'", provider.name());
            let sync_dir = provider.get_sync_directory()?;
            debug!("Sync directory: '{}'", sync_dir);
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

            debug!("Destroying VM: vm_name='{}', provider='{}'", vm_name, provider.name());

            // Initialize progress reporter
            let progress = ProgressReporter::new();

            // Show confirmation prompt
            progress.phase_header("🗑️", "DESTROY PHASE");
            let confirmation_msg = format!("├─ ⚠️  Are you sure you want to destroy {}? This will delete all data. (y/N): ", vm_name);

            if confirm_prompt(&confirmation_msg) {
                debug!("Destroy confirmation: response='yes', proceeding with destruction");
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
                debug!("Destroy confirmation: response='no', cancelling destruction");
                progress.error("└─", "Destruction cancelled");
                std::process::exit(1);
            }
        }
        Command::Ssh { path } => {
            let relative_path = path.unwrap_or_else(|| PathBuf::from("."));
            let workspace_path = config.project.as_ref()
                .and_then(|p| p.workspace_path.as_deref())
                .unwrap_or("/workspace");

            debug!("SSH command: relative_path='{}', workspace_path='{}'",
                   relative_path.display(), workspace_path);

            provider.ssh(&relative_path)
        }
        Command::Status => {
            // Enhanced status reporting using StatusFormatter
            let progress = ProgressReporter::new();
            let status_formatter = StatusFormatter::new();

            // Get VM name from config
            let vm_name = config.project.as_ref()
                .and_then(|p| p.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("vm-project");

            // Get memory and cpu info from config
            let memory = config.vm.as_ref().and_then(|vm| vm.memory);
            let cpus = config.vm.as_ref().and_then(|vm| vm.cpus);

            debug!("Status check: vm_name='{}', provider='{}', memory={:?}, cpus={:?}",
                   vm_name, provider.name(), memory, cpus);

            progress.phase_header("📊", "STATUS CHECK");
            progress.subtask("├─", "Checking VM status...");

            let result = provider.status();
            match result {
                Ok(()) => {
                    debug!("Status check successful for VM '{}'", vm_name);
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
                    debug!("Status check failed for VM '{}': {}", vm_name, e);
                    progress.error("└─", &format!("Status check failed: {}", e));
                    return Err(e);
                }
            }
            result
        }
        Command::Exec { command } => {
            debug!("Executing command in VM: command={:?}, provider='{}'", command, provider.name());
            provider.exec(&command)
        },
        Command::Logs => {
            debug!("Viewing VM logs: provider='{}'", provider.name());
            provider.logs()
        },
        Command::Validate => unreachable!(), // Handled above
        Command::Init { .. } => unreachable!(), // Handled above
        Command::Preset { .. } => unreachable!(), // Handled above
        Command::Generate { .. } => unreachable!(), // Handled above
        Command::Temp { .. } | Command::Tmp { .. } => unreachable!(), // Handled above
        Command::Config { .. } => unreachable!(), // Handled above
    }
}


// Delegation functions for clean architecture

fn find_binary(name: &str) -> String {
    // Try to find binary in development/workspace location first
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            let dev_binary = parent.join(name);
            if dev_binary.exists() {
                return dev_binary.to_string_lossy().to_string();
            }
        }
    }

    // Fall back to PATH lookup
    name.to_string()
}

fn delegate_to_vm_config(args: &[&str], file: Option<&PathBuf>) -> Result<()> {
    let mut cmd_args = vec!["vm-config"];
    cmd_args.extend(args);

    let file_str;
    if let Some(f) = file {
        cmd_args.push("--file");
        file_str = f.to_string_lossy().to_string();
        cmd_args.push(&file_str);
    }

    let binary_path = find_binary("vm-config");
    let status = ProcessCommand::new(&binary_path)
        .args(&cmd_args[1..])
        .status()
        .context("Failed to execute vm-config")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn delegate_to_vm_generator(
    services: Option<&String>,
    ports: Option<u16>,
    name: Option<&String>,
    output: Option<&PathBuf>
) -> Result<()> {
    let binary_path = find_binary("vm-generator");
    let mut cmd = ProcessCommand::new(&binary_path);
    cmd.arg("generate");

    if let Some(s) = services {
        cmd.arg("--services").arg(s);
    }
    if let Some(p) = ports {
        cmd.arg("--ports").arg(p.to_string());
    }
    if let Some(n) = name {
        cmd.arg("--name").arg(n);
    }
    if let Some(o) = output {
        cmd.arg(o);
    }

    let status = cmd.status().context("Failed to execute vm-generator")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn delegate_to_vm_temp(command: &TempSubcommand) -> Result<()> {
    let binary_path = find_binary("vm-temp");
    let mut cmd = ProcessCommand::new(&binary_path);

    match command {
        TempSubcommand::Create { mounts, auto_destroy } => {
            cmd.arg("create");
            for mount in mounts {
                cmd.arg(mount);
            }
            if *auto_destroy {
                cmd.arg("--auto-destroy");
            }
        }
        TempSubcommand::Ssh => { cmd.arg("ssh"); }
        TempSubcommand::Status => { cmd.arg("status"); }
        TempSubcommand::Destroy => { cmd.arg("destroy"); }
        TempSubcommand::Mount { path, yes } => {
            cmd.arg("mount").arg(path);
            if *yes {
                cmd.arg("--yes");
            }
        }
        TempSubcommand::Unmount { path, all, yes } => {
            cmd.arg("unmount");
            if let Some(p) = path {
                cmd.arg("--path").arg(p);
            }
            if *all {
                cmd.arg("--all");
            }
            if *yes {
                cmd.arg("--yes");
            }
        }
        TempSubcommand::Mounts => { cmd.arg("mounts"); }
        TempSubcommand::List => { cmd.arg("list"); }
        TempSubcommand::Stop => { cmd.arg("stop"); }
        TempSubcommand::Start => { cmd.arg("start"); }
        TempSubcommand::Restart => { cmd.arg("restart"); }
    }

    let status = cmd.status().context("Failed to execute vm-temp")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn delegate_to_vm_config_for_config(command: &ConfigSubcommand) -> Result<()> {
    let binary_path = find_binary("vm-config");
    let mut cmd = ProcessCommand::new(&binary_path);

    match command {
        ConfigSubcommand::Set { field, value, global } => {
            cmd.arg("set").arg(field).arg(value);
            if *global {
                cmd.arg("--global");
            }
        }
        ConfigSubcommand::Get { field, global } => {
            cmd.arg("get");
            if let Some(f) = field {
                cmd.arg(f);
            }
            if *global {
                cmd.arg("--global");
            }
        }
        ConfigSubcommand::Unset { field, global } => {
            cmd.arg("unset").arg(field);
            if *global {
                cmd.arg("--global");
            }
        }
        ConfigSubcommand::Clear { global } => {
            cmd.arg("clear");
            if *global {
                cmd.arg("--global");
            }
        }
        ConfigSubcommand::Preset { names, global, list, show } => {
            cmd.arg("preset");
            if *list {
                cmd.arg("--list");
            } else if let Some(show_name) = show {
                cmd.arg("--show").arg(show_name);
            } else if let Some(preset_names) = names {
                cmd.arg(preset_names);
            }
            if *global {
                cmd.arg("--global");
            }
        }
    }

    let status = cmd.status().context("Failed to execute vm-config")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn delegate_to_vm_config_for_preset(command: &PresetSubcommand) -> Result<()> {
    let binary_path = find_binary("vm-config");
    let mut cmd = ProcessCommand::new(&binary_path);
    cmd.arg("preset");

    match command {
        PresetSubcommand::List => {
            cmd.arg("--list");
        }
        PresetSubcommand::Show { name } => {
            cmd.arg("--detect-only").arg("--dir").arg(".").arg(name);
        }
    }

    let status = cmd.status().context("Failed to execute vm-config")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
