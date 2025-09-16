// Standard library
use std::path::PathBuf;

// External crates
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use log::{debug, info, warn};
use uuid::Uuid;

// Internal imports
use vm_common::{log_context, scoped_context, vm_error, vm_println, vm_success};
use vm_config::{config::VmConfig, init_config_file, ConfigOps};
use vm_provider::get_provider;
use vm_provider::progress::{confirm_prompt, ProgressReporter, StatusFormatter};

// Request ID for this execution - used for tracing logs across the entire request
static REQUEST_ID: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| Uuid::new_v4().to_string());

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

        /// Comma-separated services to enable (postgresql,redis,mongodb,docker)
        #[arg(long)]
        services: Option<String>,

        /// Starting port for service allocation (allocates sequential ports)
        #[arg(long)]
        ports: Option<u16>,
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
    Kill {
        /// Optional container name or ID to kill. If not provided, kills the current project's container.
        container: Option<String>,
    },
    /// Get workspace directory
    #[command(hide = true)]
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
    /// Manage configuration settings (basic operations - use 'vm-config' tool for advanced features)
    Config {
        #[command(subcommand)]
        command: ConfigSubcommand,
    },
    /// Temporary VM operations
    Temp {
        #[command(subcommand)]
        command: TempSubcommand,
    },
}

/// Load configuration with lenient validation for commands that don't require full project setup
fn load_config_lenient(file: Option<PathBuf>, _no_preset: bool) -> Result<VmConfig> {
    use vm_config::config::VmConfig;

    // Try to load defaults as base
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../defaults.yaml");
    let mut config: VmConfig =
        serde_yaml::from_str(EMBEDDED_DEFAULTS).context("Failed to parse embedded defaults")?;

    // Try to find and load user config if it exists
    let user_config_path = match file {
        Some(path) => Some(path),
        None => {
            // Look for vm.yaml in current directory
            let current_dir = std::env::current_dir()?;
            let vm_yaml_path = current_dir.join("vm.yaml");
            if vm_yaml_path.exists() {
                Some(vm_yaml_path)
            } else {
                None
            }
        }
    };

    if let Some(path) = user_config_path {
        match VmConfig::from_file(&path) {
            Ok(user_config) => {
                // Merge user config into defaults using available public API
                // For lenient loading, we'll do a simple field-by-field merge
                if user_config.provider.is_some() {
                    config.provider = user_config.provider;
                }
                if user_config.project.is_some() {
                    config.project = user_config.project;
                }
                if user_config.vm.is_some() {
                    config.vm = user_config.vm;
                }
                // Copy other important fields
                if !user_config.services.is_empty() {
                    config.services = user_config.services;
                }
            }
            Err(e) => {
                debug!("Failed to load user config, using defaults: {}", e);
            }
        }
    }

    // Ensure we have at least a minimal valid config for providers
    if config.provider.is_none() {
        config.provider = Some(String::from("docker"));
    }

    Ok(config)
}

fn main() -> Result<()> {
    // Initialize structured logging system first, but only if not in test mode
    // Tests expect clean stdout output, so we disable logging for test runs
    if std::env::var("VM_TEST_MODE").is_err() {
        if vm_common::logging::init().is_err() {
            eprintln!(
                "Warning: Failed to initialize structured logging, falling back to basic logging"
            );
        }
    }

    let args = Args::parse();

    // Set up request-level context that will be inherited by all logs
    let _request_guard = scoped_context! {
        "request_id" => REQUEST_ID.as_str(),
        "command" => format!("{:?}", args.command),
        "debug" => args.debug
    };

    info!("Starting vm command");

    // For commands that don't need a provider, handle them first.
    match &args.command {
        Command::Validate => {
            debug!(
                "Validating configuration: config_file={:?}, no_preset={}",
                args.config, args.no_preset
            );
            // The `load` function performs validation internally. If it succeeds,
            // the configuration is valid.
            match VmConfig::load(args.config, args.no_preset) {
                Ok(config) => {
                    debug!(
                        "Configuration validation successful: provider={:?}, project_name={:?}",
                        config.provider,
                        config.project.as_ref().and_then(|p| p.name.as_ref())
                    );
                    vm_success!("Configuration is valid.");
                    return Ok(());
                }
                Err(e) => {
                    debug!("Configuration validation failed: {}", e);
                    vm_error!("Configuration is invalid: {:#}", e);
                    // Return the error to exit with a non-zero status code
                    return Err(e);
                }
            }
        }
        Command::Init {
            file,
            services,
            ports,
        } => {
            debug!("Calling init_config_file directly");
            return init_config_file(file.clone(), services.clone(), *ports);
        }
        Command::Config { command } => {
            debug!("Calling ConfigOps methods directly");
            return handle_config_command(command);
        }
        Command::Preset { command } => {
            debug!("Calling preset methods directly");
            return handle_preset_command(command);
        }
        Command::Temp { command } => {
            debug!("Calling temp VM operations directly");
            return handle_temp_command(command, args.config, args.no_preset);
        }
        _ => {} // Continue to provider-based commands
    }

    // Handle dry-run for provider commands
    if args.dry_run {
        match &args.command {
            Command::Create
            | Command::Start
            | Command::Stop
            | Command::Restart
            | Command::Destroy
            | Command::Provision
            | Command::Kill { .. } => {
                vm_println!("🔍 DRY RUN MODE - showing what would be executed:");
                vm_println!("   Command: {:?}", args.command);
                if let Some(config) = &args.config {
                    vm_println!("   Config: {}", config.display());
                }
                if let Some(preset) = &args.preset {
                    vm_println!("   Preset override: {}", preset);
                }
                vm_println!("🚫 Dry run complete - no commands were executed");
                return Ok(());
            }
            _ => {} // Non-provider commands proceed normally
        }
    }

    // 1. Load configuration
    // The vm-config crate now handles file discovery, preset merging, and validation.
    debug!(
        "Loading configuration: config_file={:?}, no_preset={}, preset_override={:?}",
        args.config, args.no_preset, args.preset
    );

    let config = if let Some(preset) = args.preset {
        debug!("Using preset override: {}", preset);
        VmConfig::load_with_preset(args.config, preset)?
    } else {
        // For List command, try lenient loading first to avoid validation errors
        if matches!(args.command, Command::List) {
            match load_config_lenient(args.config.clone(), args.no_preset) {
                Ok(config) => config,
                Err(_) => {
                    // If lenient loading fails, fall back to strict loading
                    VmConfig::load(args.config, args.no_preset)?
                }
            }
        } else {
            VmConfig::load(args.config, args.no_preset)?
        }
    };

    debug!(
        "Loaded configuration: provider={:?}, project_name={:?}",
        config.provider,
        config.project.as_ref().and_then(|p| p.name.as_ref())
    );

    // 2. Get the appropriate provider
    let provider = get_provider(config.clone())?;

    // Add provider context that will be inherited by all subsequent logs
    log_context! {
        "provider" => provider.name()
    };

    debug!("Using provider: {}", provider.name());

    // 3. Execute the command
    debug!("Executing command: {:?}", args.command);
    match args.command {
        Command::Create => {
            let _op_guard = scoped_context! { "operation" => "create" };
            info!("Starting VM creation");
            provider.create()
        }
        Command::Start => {
            let _op_guard = scoped_context! { "operation" => "start" };
            info!("Starting VM");
            provider.start()
        }
        Command::Stop => {
            let _op_guard = scoped_context! { "operation" => "stop" };
            info!("Stopping VM");
            provider.stop()
        }
        Command::Restart => {
            let _op_guard = scoped_context! { "operation" => "restart" };
            info!("Restarting VM");
            provider.restart()
        }
        Command::Provision => {
            let _op_guard = scoped_context! { "operation" => "provision" };
            info!("Re-running VM provisioning");
            provider.provision()
        }
        Command::List => {
            let _op_guard = scoped_context! { "operation" => "list" };
            debug!("Listing VMs");
            provider.list()
        }
        Command::Kill { container } => {
            let _op_guard = scoped_context! { "operation" => "kill" };
            warn!("Force killing VM processes: container={:?}", container);
            provider.kill(container.as_deref())
        }
        Command::GetSyncDirectory => {
            debug!("Getting sync directory for provider '{}'", provider.name());
            let sync_dir = provider.get_sync_directory()?;
            debug!("Sync directory: '{}'", sync_dir);
            println!("{}", sync_dir);
            Ok(())
        }
        Command::Destroy => {
            // Get VM name from config for confirmation prompt
            let vm_name = config
                .project
                .as_ref()
                .and_then(|p| p.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("VM");

            debug!(
                "Destroying VM: vm_name='{}', provider='{}'",
                vm_name,
                provider.name()
            );

            // Initialize progress reporter
            let progress = ProgressReporter::new();

            // Show confirmation prompt
            progress.phase_header("🗑️", "DESTROY PHASE");
            let confirmation_msg = format!(
                "├─ ⚠️  Are you sure you want to destroy {}? This will delete all data. (y/N): ",
                vm_name
            );

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
            let workspace_path = config
                .project
                .as_ref()
                .and_then(|p| p.workspace_path.as_deref())
                .unwrap_or("/workspace");

            debug!(
                "SSH command: relative_path='{}', workspace_path='{}'",
                relative_path.display(),
                workspace_path
            );

            provider.ssh(&relative_path)
        }
        Command::Status => {
            // Enhanced status reporting using StatusFormatter
            let progress = ProgressReporter::new();
            let status_formatter = StatusFormatter::new();

            // Get VM name from config
            let vm_name = config
                .project
                .as_ref()
                .and_then(|p| p.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("vm-project");

            // Get memory and cpu info from config
            let memory = config.vm.as_ref().and_then(|vm| vm.memory);
            let cpus = config.vm.as_ref().and_then(|vm| vm.cpus);

            debug!(
                "Status check: vm_name='{}', provider='{}', memory={:?}, cpus={:?}",
                vm_name,
                provider.name(),
                memory,
                cpus
            );

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
                        cpus,
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
            debug!(
                "Executing command in VM: command={:?}, provider='{}'",
                command,
                provider.name()
            );
            provider.exec(&command)
        }
        Command::Logs => {
            debug!("Viewing VM logs: provider='{}'", provider.name());
            provider.logs()
        }
        Command::Validate => unreachable!(),    // Handled above
        Command::Init { .. } => unreachable!(), // Handled above
        Command::Preset { .. } => unreachable!(), // Handled above
        Command::Config { .. } => unreachable!(), // Handled above
        Command::Temp { .. } => unreachable!(), // Handled above
    }
}

// Direct function call handlers for config operations

fn handle_config_command(command: &ConfigSubcommand) -> Result<()> {
    match command {
        ConfigSubcommand::Set {
            field,
            value,
            global,
        } => ConfigOps::set(field, value, *global),
        ConfigSubcommand::Get { field, global } => ConfigOps::get(field.as_deref(), *global),
        ConfigSubcommand::Unset { field, global } => ConfigOps::unset(field, *global),
        ConfigSubcommand::Clear { global } => ConfigOps::clear(*global),
        ConfigSubcommand::Preset {
            names,
            global,
            list,
            show,
        } => match (list, show, names) {
            (true, _, _) => ConfigOps::preset("", *global, true, None),
            (_, Some(show_name), _) => ConfigOps::preset("", *global, false, Some(show_name)),
            (_, _, Some(preset_names)) => ConfigOps::preset(preset_names, *global, false, None),
            _ => Ok(()),
        },
    }
}

fn handle_preset_command(command: &PresetSubcommand) -> Result<()> {
    match command {
        PresetSubcommand::List => ConfigOps::preset("", false, true, None),
        PresetSubcommand::Show { name } => ConfigOps::preset("", false, false, Some(name)),
    }
}

fn handle_temp_command(
    command: &TempSubcommand,
    config_file: Option<PathBuf>,
    no_preset: bool,
) -> Result<()> {
    use vm_temp::TempVmOps;

    // For temp commands, we need a provider, but the config might not exist.
    // We load it leniently to ensure we can get a provider.
    let config = load_config_lenient(config_file, no_preset)?;
    let provider = get_provider(config.clone())?;

    match command {
        TempSubcommand::Create {
            mounts,
            auto_destroy,
        } => TempVmOps::create(mounts.clone(), *auto_destroy, config, provider),
        TempSubcommand::Ssh => TempVmOps::ssh(provider),
        TempSubcommand::Status => TempVmOps::status(provider),
        TempSubcommand::Destroy => TempVmOps::destroy(provider),
        TempSubcommand::Mount { path, yes } => TempVmOps::mount(path.clone(), *yes, provider),
        TempSubcommand::Unmount { path, all, yes } => {
            TempVmOps::unmount(path.clone(), *all, *yes, provider)
        }
        TempSubcommand::Mounts => TempVmOps::mounts(),
        TempSubcommand::List => TempVmOps::list(),
        TempSubcommand::Stop => TempVmOps::stop(provider),
        TempSubcommand::Start => TempVmOps::start(provider),
        TempSubcommand::Restart => TempVmOps::restart(provider),
    }
}
