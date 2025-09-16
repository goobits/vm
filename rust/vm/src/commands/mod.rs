// Command handlers for VM operations

use anyhow::Result;
use log::debug;
// Import the CLI types
use crate::cli::{Args, Command};
use vm_common::{log_context, vm_println};
use vm_config::{config::VmConfig, init_config_file};
use vm_provider::get_provider;

// Individual command modules
pub mod config;
pub mod preset;
pub mod temp;
pub mod vm_ops;

/// Main command dispatcher
pub fn execute_command(args: Args) -> Result<()> {
    // Handle dry-run for provider commands
    if args.dry_run {
        return handle_dry_run(&args);
    }

    // Handle commands that don't need a provider first
    match &args.command {
        Command::Validate => config::handle_validate(args.config, args.no_preset),
        Command::Init {
            file,
            services,
            ports,
        } => {
            debug!("Calling init_config_file directly");
            init_config_file(file.clone(), services.clone(), *ports)
        }
        Command::Config { command } => {
            debug!("Calling ConfigOps methods directly");
            config::handle_config_command(command, args.dry_run)
        }
        Command::Preset { command } => {
            debug!("Calling preset methods directly");
            preset::handle_preset_command(command)
        }
        Command::Temp { command } => {
            debug!("Calling temp VM operations directly");
            temp::handle_temp_command(command, args.config, args.no_preset)
        }
        _ => {
            // Provider-based commands
            handle_provider_command(args)
        }
    }
}

fn handle_dry_run(args: &Args) -> Result<()> {
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
            Ok(())
        }
        _ => {
            // Non-provider commands proceed normally
            let mut args_copy = args.clone();
            args_copy.dry_run = false;
            execute_command(args_copy)
        }
    }
}

fn handle_provider_command(args: Args) -> Result<()> {
    // Load configuration
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
            match config::load_config_lenient(args.config.clone(), args.no_preset) {
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

    // Get the appropriate provider
    let provider = get_provider(config.clone())?;

    // Add provider context that will be inherited by all subsequent logs
    log_context! {
        "provider" => provider.name()
    };

    debug!("Using provider: {}", provider.name());

    // Execute the command
    debug!("Executing command: {:?}", args.command);
    match args.command {
        Command::Create => vm_ops::handle_create(provider),
        Command::Start => vm_ops::handle_start(provider),
        Command::Stop => vm_ops::handle_stop(provider),
        Command::Restart => vm_ops::handle_restart(provider),
        Command::Provision => vm_ops::handle_provision(provider),
        Command::List => vm_ops::handle_list(provider),
        Command::Kill { container } => vm_ops::handle_kill(provider, container),
        Command::GetSyncDirectory => vm_ops::handle_get_sync_directory(provider),
        Command::Destroy => vm_ops::handle_destroy(provider, config),
        Command::Ssh { path } => vm_ops::handle_ssh(provider, path, config),
        Command::Status => vm_ops::handle_status(provider, config),
        Command::Exec { command } => vm_ops::handle_exec(provider, command),
        Command::Logs => vm_ops::handle_logs(provider),
        _ => unreachable!(), // Other commands handled above
    }
}
