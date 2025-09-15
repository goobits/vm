use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::fs;
use vm_config::config::VmConfig;
use vm_config::preset::PresetDetector;
use vm_config::paths;
use vm_provider::get_provider;
use vm_provider::progress::{confirm_prompt, ProgressReporter, StatusFormatter};
use vm_temp::{TempVmState, StateManager, MountPermission, MountParser};

// Global debug flag
static mut DEBUG_ENABLED: bool = false;

// Debug output macro
macro_rules! debug {
    ($($arg:tt)*) => {
        if unsafe { DEBUG_ENABLED } {
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
    unsafe {
        DEBUG_ENABLED = args.debug;
    }

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
            debug!("Initializing configuration file: custom_path={:?}", file);
            // Initialize a new vm.yaml configuration file
            let result = vm_config::cli::init_config_file(file.clone());
            match &result {
                Ok(_) => debug!("Configuration file initialization successful"),
                Err(e) => debug!("Configuration file initialization failed: {}", e),
            }
            return result;
        }
        Command::Generate { services, ports, name, output } => {
            return handle_generate_command(services.clone(), ports.clone(), name.clone(), output.clone());
        }
        Command::Temp { command } | Command::Tmp { command } => {
            return handle_temp_command(command);
        }
        Command::Config { command } => {
            debug!("Config command: {:?}", command);
            let result = handle_config_command(command);
            match &result {
                Ok(_) => debug!("Config command completed successfully"),
                Err(e) => debug!("Config command failed: {}", e),
            }
            return result;
        }
        Command::Preset { command } => {
            // Handle preset commands
            let project_dir = std::env::current_dir()?;
            let presets_dir = paths::get_presets_dir();
            debug!("Preset command: project_dir={:?}, presets_dir={:?}", project_dir, presets_dir);
            let detector = PresetDetector::new(project_dir, presets_dir);

            match command {
                PresetSubcommand::List => {
                    debug!("Listing available presets");
                    println!("Available presets:");
                    let presets = detector.list_presets()?;
                    debug!("Found {} presets", presets.len());
                    for preset in presets {
                        println!("  {}", preset);
                    }
                    return Ok(());
                }
                PresetSubcommand::Show { name } => {
                    debug!("Showing preset configuration: name='{}'", name);
                    let config = detector.load_preset(name)?;
                    let yaml = serde_yaml::to_string(&config)?;
                    debug!("Successfully loaded preset '{}' configuration", name);
                    println!("Preset '{}' configuration:", name);
                    println!("{}", yaml);
                    return Ok(());
                }
            }
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

// Generate command implementation
fn handle_generate_command(
    services: Option<String>,
    ports: Option<u16>,
    name: Option<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    debug!("Generate command: services={:?}, ports={:?}, name={:?}, output={:?}",
           services, ports, name, output);
    println!("⚙️ Generating configuration...");

    // Load base config from defaults
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../defaults.yaml");
    let mut config: VmConfig = serde_yaml::from_str(EMBEDDED_DEFAULTS)
        .context("Failed to parse embedded defaults")?;

    // Parse and merge services
    if let Some(ref services_str) = services {
        let service_list: Vec<String> = services_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        for service in service_list {
            debug!("Loading service config: {}", service);
            // Load service config
            let service_path = paths::resolve_tool_path(format!("configs/services/{}.yaml", service));
            debug!("Service config path: {:?}", service_path);
            if !service_path.exists() {
                eprintln!("❌ Unknown service: {}", service);
                eprintln!("💡 Available services: postgresql, redis, mongodb, docker");
                return Err(anyhow::anyhow!("Service configuration not found"));
            }

            let service_config = VmConfig::from_file(&service_path)
                .with_context(|| format!("Failed to load service config: {}", service))?;
            debug!("Loaded service config for: {}", service);

            // Merge service config into base
            config = vm_config::merge::ConfigMerger::new(config)
                .merge(service_config)?;
            debug!("Merged service config: {}", service);
        }
    }

    // Apply port configuration
    if let Some(port_start) = ports {
        if port_start < 1024 || port_start > 65535 {
            return Err(anyhow::anyhow!("Invalid port number: {} (must be between 1024-65535)", port_start));
        }

        // Allocate sequential ports
        config.ports.insert("web".to_string(), port_start);
        config.ports.insert("api".to_string(), port_start + 1);
        config.ports.insert("postgresql".to_string(), port_start + 5);
        config.ports.insert("redis".to_string(), port_start + 6);
        config.ports.insert("mongodb".to_string(), port_start + 7);
    }

    // Apply project name
    if let Some(ref project_name) = name {
        if let Some(ref mut project) = config.project {
            project.name = Some(project_name.clone());
            project.hostname = Some(format!("dev.{}.local", project_name));
        }
        if let Some(ref mut terminal) = config.terminal {
            terminal.username = Some(format!("{}-dev", project_name));
        }
    }

    // Write to output file
    let output_path = output.unwrap_or_else(|| PathBuf::from("vm.yaml"));
    let yaml = serde_yaml::to_string(&config)?;
    fs::write(&output_path, yaml).context("Failed to write configuration file")?;

    println!("✅ Generated {}", output_path.display());
    if let Some(ref services_str) = services {
        println!("   Services: {}", services_str);
    }
    if let Some(port_start) = ports {
        println!("   Port range: {}-{}", port_start, port_start + 9);
    }
    if let Some(ref project_name) = name {
        println!("   Project name: {}", project_name);
    }

    Ok(())
}

// Temp command implementation
fn handle_temp_command(command: &TempSubcommand) -> Result<()> {
    debug!("Temp command: {:?}", command);
    let state_manager = StateManager::new()
        .context("Failed to initialize state manager")?;

    match command {
        TempSubcommand::Create { mounts, auto_destroy } => {
            // Parse mount strings using MountParser
            let parsed_mounts = MountParser::parse_mount_strings(mounts)
                .context("Failed to parse mount strings")?;

            // Get current project directory
            let project_dir = std::env::current_dir()
                .context("Failed to get current directory")?;

            // Create temp VM state
            let mut temp_state = TempVmState::new(
                "vm-temp-dev".to_string(),
                "docker".to_string(),
                project_dir,
                *auto_destroy,
            );

            // Add all mounts to the state
            for (source, target, permissions) in parsed_mounts {
                if let Some(target_path) = target {
                    temp_state.add_mount_with_target(source, target_path, permissions)
                        .context("Failed to add mount with custom target")?;
                } else {
                    temp_state.add_mount(source, permissions)
                        .context("Failed to add mount")?;
                }
            }

            // Create minimal temp config
            let temp_config = create_temp_config()?;

            // Create the VM
            println!("🚀 Creating temporary VM...");
            let provider = get_provider(temp_config)?;
            provider.create()?;

            // Save state
            state_manager.save_state(&temp_state)
                .context("Failed to save temp VM state")?;

            println!("✅ Temporary VM created with {} mount(s)", temp_state.mount_count());

            if *auto_destroy {
                // SSH then destroy
                println!("🔗 Connecting to temporary VM...");
                provider.ssh(&PathBuf::from("."))?;

                println!("🗑️ Auto-destroying temporary VM...");
                provider.destroy()?;
                state_manager.delete_state()
                    .context("Failed to delete temp VM state")?;
            } else {
                println!("💡 Use 'vm temp ssh' to connect");
                println!("   Use 'vm temp destroy' when done");
            }

            Ok(())
        }
        TempSubcommand::Ssh => {
            if !state_manager.state_exists() {
                return Err(anyhow::anyhow!("No temp VM found\n💡 Create one with: vm temp create ./your-directory"));
            }

            let temp_config = create_temp_config()?;
            let provider = get_provider(temp_config)?;
            provider.ssh(&PathBuf::from("."))
        }
        TempSubcommand::Status => {
            if !state_manager.state_exists() {
                println!("❌ No temp VM found");
                println!("💡 Create one with: vm temp create ./your-directory");
                return Ok(());
            }

            let state = state_manager.load_state()
                .context("Failed to load temp VM state")?;

            println!("📊 Temp VM Status:");
            println!("   Container: {}", state.container_name);
            println!("   Provider: {}", state.provider);
            println!("   Created: {}", state.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
            println!("   Project: {}", state.project_dir.display());
            println!("   Mounts: {}", state.mount_count());
            if state.is_auto_destroy() {
                println!("   Auto-destroy: enabled");
            }

            // Check provider status
            let temp_config = create_temp_config()?;
            let provider = get_provider(temp_config)?;
            provider.status()
        }
        TempSubcommand::Destroy => {
            if !state_manager.state_exists() {
                return Err(anyhow::anyhow!("No temp VM found"));
            }

            let temp_config = create_temp_config()?;
            let provider = get_provider(temp_config)?;

            println!("🗑️ Destroying temporary VM...");
            provider.destroy()?;
            state_manager.delete_state()
                .context("Failed to delete temp VM state")?;

            println!("✅ Temporary VM destroyed");
            Ok(())
        }
        TempSubcommand::Mount { path, yes } => {
            if !state_manager.state_exists() {
                return Err(anyhow::anyhow!("No temp VM found. Create one first with: vm temp create"));
            }

            // Parse the mount string
            let (source, target, permissions) = MountParser::parse_mount_string(path)
                .context("Failed to parse mount string")?;

            // Load current state
            let mut state = state_manager.load_state()
                .context("Failed to load temp VM state")?;

            // Check if mount already exists
            if state.has_mount(&source) {
                return Err(anyhow::anyhow!("Mount already exists for source: {}", source.display()));
            }

            // Confirm action unless --yes flag is used
            if !yes {
                let confirmation_msg = format!("Add mount {} to temp VM? (y/N): ", source.display());
                if !confirm_prompt(&confirmation_msg) {
                    println!("❌ Mount operation cancelled");
                    return Ok(());
                }
            }

            // Add the mount
            let permissions_display = permissions.to_string();
            if let Some(target_path) = target {
                state.add_mount_with_target(source.clone(), target_path, permissions)
                    .context("Failed to add mount with custom target")?;
            } else {
                state.add_mount(source.clone(), permissions)
                    .context("Failed to add mount")?;
            }

            // Save updated state
            state_manager.save_state(&state)
                .context("Failed to save updated temp VM state")?;

            println!("🔗 Mount added: {} ({})", source.display(), permissions_display);
            println!("💡 VM will need to be recreated for mount to take effect");
            println!("   Use 'vm temp restart' to apply changes");

            Ok(())
        }
        TempSubcommand::Unmount { path, all, yes } => {
            if !state_manager.state_exists() {
                return Err(anyhow::anyhow!("No temp VM found"));
            }

            // Load current state
            let mut state = state_manager.load_state()
                .context("Failed to load temp VM state")?;

            if *all {
                if !yes {
                    let confirmation_msg = format!("Remove all {} mounts from temp VM? (y/N): ", state.mount_count());
                    if !confirm_prompt(&confirmation_msg) {
                        println!("❌ Unmount operation cancelled");
                        return Ok(());
                    }
                }

                let mount_count = state.mount_count();
                state.clear_mounts();

                // Save updated state
                state_manager.save_state(&state)
                    .context("Failed to save updated temp VM state")?;

                println!("🗑️ Removed all {} mount(s)", mount_count);
            } else if let Some(path_str) = path {
                let source_path = PathBuf::from(path_str);

                if !state.has_mount(&source_path) {
                    return Err(anyhow::anyhow!("Mount not found for source: {}", source_path.display()));
                }

                if !yes {
                    let confirmation_msg = format!("Remove mount {} from temp VM? (y/N): ", source_path.display());
                    if !confirm_prompt(&confirmation_msg) {
                        println!("❌ Unmount operation cancelled");
                        return Ok(());
                    }
                }

                let removed_mount = state.remove_mount(&source_path)
                    .context("Failed to remove mount")?;

                // Save updated state
                state_manager.save_state(&state)
                    .context("Failed to save updated temp VM state")?;

                println!("🗑️ Removed mount: {} ({})", removed_mount.source.display(), removed_mount.permissions);
            } else {
                return Err(anyhow::anyhow!("Must specify --path or --all"));
            }

            println!("💡 VM will need to be recreated for changes to take effect");
            println!("   Use 'vm temp restart' to apply changes");

            Ok(())
        }
        TempSubcommand::Mounts => {
            if !state_manager.state_exists() {
                println!("❌ No temp VM found");
                return Ok(());
            }

            let state = state_manager.load_state()
                .context("Failed to load temp VM state")?;

            if state.mount_count() == 0 {
                println!("📁 No mounts configured");
                return Ok(());
            }

            println!("📁 Current mounts ({}):", state.mount_count());
            for mount in state.get_mounts() {
                println!("   {} → {} ({})",
                    mount.source.display(),
                    mount.target.display(),
                    mount.permissions
                );
            }

            // Show mount summary by permission
            let ro_count = state.mount_count_by_permission(MountPermission::ReadOnly);
            let rw_count = state.mount_count_by_permission(MountPermission::ReadWrite);

            println!("   {} read-only, {} read-write", ro_count, rw_count);

            Ok(())
        }
        TempSubcommand::List => {
            // For now, just show if there's a temp VM
            if state_manager.state_exists() {
                let state = state_manager.load_state()
                    .context("Failed to load temp VM state")?;

                println!("📋 Temp VMs:");
                println!("   {} ({})", state.container_name, state.provider);
                println!("      Created: {}", state.created_at.format("%Y-%m-%d %H:%M:%S"));
                println!("      Project: {}", state.project_dir.display());
                println!("      Mounts: {}", state.mount_count());
            } else {
                println!("📋 No temp VMs found");
            }

            Ok(())
        }
        TempSubcommand::Stop => {
            println!("⏸️  Stop command not yet implemented");
            println!("💡 Use 'vm temp destroy' to remove the VM completely");
            Ok(())
        }
        TempSubcommand::Start => {
            println!("▶️  Start command not yet implemented");
            println!("💡 Use 'vm temp create' to create a new temp VM");
            Ok(())
        }
        TempSubcommand::Restart => {
            println!("🔄 Restart command not yet implemented");
            println!("💡 Use 'vm temp destroy' then 'vm temp create' for now");
            Ok(())
        }
    }
}

// Helper function to create temp VM config
fn create_temp_config() -> Result<VmConfig> {
    let mut config = VmConfig::default();
    config.provider = Some("docker".to_string());

    if let Some(ref mut project) = config.project {
        project.name = Some("vm-temp".to_string());
        project.hostname = Some("vm-temp.local".to_string());
        project.workspace_path = Some("/workspace".to_string());
    } else {
        config.project = Some(vm_config::config::ProjectConfig {
            name: Some("vm-temp".to_string()),
            hostname: Some("vm-temp.local".to_string()),
            workspace_path: Some("/workspace".to_string()),
            backup_pattern: None,
            env_template_path: None,
        });
    }

    Ok(config)
}


// Config command implementation
fn handle_config_command(command: &ConfigSubcommand) -> Result<()> {
    use vm_config::config_ops::ConfigOps;

    match command {
        ConfigSubcommand::Set { field, value, global } => {
            debug!("Config set: field='{}', value='{}', global={}", field, value, global);
            ConfigOps::set(field, value, *global)
        }
        ConfigSubcommand::Get { field, global } => {
            debug!("Config get: field={:?}, global={}", field, global);
            ConfigOps::get(field.as_deref(), *global)
        }
        ConfigSubcommand::Unset { field, global } => {
            debug!("Config unset: field='{}', global={}", field, global);
            ConfigOps::unset(field, *global)
        }
        ConfigSubcommand::Clear { global } => {
            debug!("Config clear: global={}", global);
            ConfigOps::clear(*global)
        }
        ConfigSubcommand::Preset { names, global, list, show } => {
            if *list {
                debug!("Config preset: list=true");
                ConfigOps::preset("", false, true, None)
            } else if let Some(show_name) = show {
                debug!("Config preset: show='{}'", show_name);
                ConfigOps::preset("", false, false, Some(show_name))
            } else if let Some(preset_names) = names {
                debug!("Config preset: apply names='{}', global={}", preset_names, global);
                ConfigOps::preset(preset_names, *global, false, None)
            } else {
                debug!("Config preset: invalid arguments - no names, list, or show specified");
                anyhow::bail!("Must specify preset names, --list, or --show");
            }
        }
    }
}
