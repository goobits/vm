// Command handlers for VM operations

use anyhow::Result;
use log::debug;
// Import the CLI types
use crate::cli::{Args, Command};
use vm_common::{log_context, vm_error, vm_println};
use vm_config::{config::VmConfig, init_config_file};
use vm_provider::{error::ProviderError, get_provider};

// Individual command modules
pub mod config;
pub mod temp;
pub mod vm_ops;

/// Main command dispatcher
#[must_use = "command execution results should be handled"]
pub fn execute_command(args: Args) -> Result<()> {
    // Handle dry-run for provider commands
    if args.dry_run {
        return handle_dry_run(&args);
    }

    // Handle commands that don't need a provider first
    match &args.command {
        Command::Validate => config::handle_validate(args.config),
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
        Command::Temp { command } => {
            debug!("Calling temp VM operations directly");
            temp::handle_temp_command(command, args.config)
        }
        _ => {
            // Provider-based commands
            handle_provider_command(args)
        }
    }
}

fn handle_dry_run(args: &Args) -> Result<()> {
    match &args.command {
        Command::Create { .. }
        | Command::Start
        | Command::Stop { .. }
        | Command::Restart
        | Command::Destroy { .. }
        | Command::Provision => {
            vm_println!("🔍 DRY RUN MODE - showing what would be executed:");
            vm_println!("   Command: {:?}", args.command);
            if let Some(config) = &args.config {
                vm_println!("   Config: {}", config.display());
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
    debug!("Loading configuration: config_file={:?}", args.config);

    let config = {
        // For List command, try lenient loading first to avoid validation errors
        if matches!(args.command, Command::List) {
            match config::load_config_lenient(args.config.clone()) {
                Ok(config) => config,
                Err(_) => {
                    // If lenient loading fails, fall back to strict loading
                    VmConfig::load(args.config)?
                }
            }
        } else {
            VmConfig::load(args.config)?
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

    // Execute the command with friendly error handling
    debug!("Executing command: {:?}", args.command);
    let result = match args.command {
        Command::Create { force } => vm_ops::handle_create(provider, force),
        Command::Start => vm_ops::handle_start(provider, config.clone()),
        Command::Stop { container } => vm_ops::handle_stop(provider, container, config.clone()),
        Command::Restart => vm_ops::handle_restart(provider, config.clone()),
        Command::Provision => vm_ops::handle_provision(provider, config.clone()),
        Command::List => vm_ops::handle_list(provider),
        Command::GetSyncDirectory => {
            vm_ops::handle_get_sync_directory(provider);
            Ok(())
        }
        Command::Destroy { force } => vm_ops::handle_destroy(provider, config, force),
        Command::Ssh { path } => vm_ops::handle_ssh(provider, path, config),
        Command::Status => vm_ops::handle_status(provider, config),
        Command::Exec { command } => vm_ops::handle_exec(provider, command, config.clone()),
        Command::Logs => vm_ops::handle_logs(provider, config.clone()),
        cmd => {
            vm_error!(
                "Command {:?} should have been handled in earlier match statement",
                cmd
            );
            Err(anyhow::anyhow!("Command not handled in match statement"))
        }
    };

    // Convert errors to user-friendly messages
    result.map_err(|e| {
        if let Some(provider_error) = e.downcast_ref::<ProviderError>() {
            anyhow::anyhow!(provider_error.user_friendly())
        } else {
            // Check if it's an anyhow error containing a ProviderError
            let error_chain = format!("{:?}", e);
            if error_chain.contains("is not running") {
                anyhow::anyhow!("🔴 VM is stopped\n🚀 Start with: vm start")
            } else if error_chain.contains("No such container") {
                anyhow::anyhow!("🔍 VM doesn't exist\n💡 Create with: vm create")
            } else if error_chain.contains("SSH command failed")
                || error_chain.contains("exited with code 1")
            {
                anyhow::anyhow!("🔌 Cannot connect to VM\n📊 Check status: vm status")
            } else {
                e
            }
        }
    })
}
