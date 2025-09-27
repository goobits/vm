// Command handlers for VM operations

use anyhow::Result;
use tracing::debug;
// Import the CLI types
use crate::cli::{Args, Command};
use vm_common::{vm_error, vm_println};
use vm_config::{config::VmConfig, init_config_file};
use vm_provider::{error::ProviderError, get_provider};

// Individual command modules
pub mod config;
pub mod pkg;
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
        Command::Pkg { command } => {
            debug!("Calling package registry operations");
            tokio::runtime::Runtime::new()?
                .block_on(async { pkg::handle_pkg_command(command, args.config).await })
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
        | Command::Start { .. }
        | Command::Stop { .. }
        | Command::Restart { .. }
        | Command::Destroy { .. }
        | Command::Provision { .. } => {
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
        if matches!(args.command, Command::List { .. }) {
            match config::load_config_lenient(args.config.clone()) {
                Ok(config) => config,
                Err(_) => {
                    // If lenient loading fails, fall back to strict loading
                    match VmConfig::load(args.config) {
                        Ok(config) => config,
                        Err(e) => {
                            let error_str = e.to_string();
                            #[allow(clippy::excessive_nesting)]
                            if error_str.contains("No vm.yaml found") {
                                println!("❌ No vm.yaml configuration file found\n");
                                println!("💡 You need a configuration file to run VMs. Try:");
                                println!("   • Initialize config: vm init");
                                println!("   • Change to project directory: cd <project>");
                                println!("   • List existing VMs: vm list --all-providers");
                                return Err(anyhow::anyhow!("Configuration required"));
                            }
                            return Err(e);
                        }
                    }
                }
            }
        } else {
            match VmConfig::load(args.config) {
                Ok(config) => config,
                Err(e) => {
                    let error_str = e.to_string();
                    #[allow(clippy::excessive_nesting)]
                    if error_str.contains("No vm.yaml found") {
                        println!("❌ No vm.yaml configuration file found\n");
                        println!("💡 You need a configuration file to run VMs. Try:");
                        println!("   • Initialize config: vm init");
                        println!("   • Change to project directory: cd <project>");
                        println!("   • List existing VMs: vm list --all-providers");
                        return Err(anyhow::anyhow!("Configuration required"));
                    }
                    return Err(e);
                }
            }
        }
    };

    debug!(
        "Loaded configuration: provider={:?}, project_name={:?}",
        config.provider,
        config.project.as_ref().and_then(|p| p.name.as_ref())
    );

    // Get the appropriate provider
    let provider = get_provider(config.clone())?;

    // Log provider being used
    debug!(provider = %provider.name(), "Using provider");

    // Execute the command with friendly error handling
    debug!("Executing command: {:?}", args.command);
    let result = match args.command {
        Command::Create { force, instance } => {
            vm_ops::handle_create(provider, config.clone(), force, instance)
        }
        Command::Start { container } => {
            vm_ops::handle_start(provider, container.as_deref(), config.clone())
        }
        Command::Stop { container } => {
            vm_ops::handle_stop(provider, container.as_deref(), config.clone())
        }
        Command::Restart { container } => {
            vm_ops::handle_restart(provider, container.as_deref(), config.clone())
        }
        Command::Provision { container } => {
            vm_ops::handle_provision(provider, container.as_deref(), config.clone())
        }
        Command::List {
            all_providers,
            provider: provider_filter,
            verbose,
        } => vm_ops::handle_list_enhanced(
            provider,
            &all_providers,
            provider_filter.as_deref(),
            &verbose,
        ),
        Command::GetSyncDirectory => {
            vm_ops::handle_get_sync_directory(provider);
            Ok(())
        }
        Command::Destroy {
            container,
            force,
            all,
            provider: provider_filter,
            pattern,
        } => vm_ops::handle_destroy_enhanced(
            provider,
            container.as_deref(),
            config,
            &force,
            &all,
            provider_filter.as_deref(),
            pattern.as_deref(),
        ),
        Command::Ssh { container, path } => {
            vm_ops::handle_ssh(provider, container.as_deref(), path, config)
        }
        Command::Status { container } => {
            vm_ops::handle_status(provider, container.as_deref(), config)
        }
        Command::Exec { container, command } => {
            vm_ops::handle_exec(provider, container.as_deref(), command, config.clone())
        }
        Command::Logs { container } => {
            vm_ops::handle_logs(provider, container.as_deref(), config.clone())
        }
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
            let error_str = e.to_string();

            // More specific error recovery suggestions
            if error_chain.contains("is not running") || error_str.contains("is not running") {
                anyhow::anyhow!("🔴 VM is stopped\n\n💡 Try:\n  • Start VM: vm start\n  • Check status: vm status\n  • View logs: vm logs")
            } else if error_chain.contains("No such container") || error_str.contains("No such container") {
                anyhow::anyhow!("🔍 VM doesn't exist\n\n💡 Try:\n  • Create VM: vm create\n  • List all VMs: vm list\n  • Check config: vm validate")
            } else if error_chain.contains("SSH connection lost") {
                // Don't show duplicate message for normal SSH exits
                e
            } else if error_chain.contains("port") || error_str.contains("port") {
                anyhow::anyhow!("⚠️ Port conflict detected\n\n💡 Try:\n  • Fix ports: vm config ports --fix\n  • Check ports: docker ps\n  • Recreate: vm create --force")
            } else if error_chain.contains("permission") || error_str.contains("permission") {
                anyhow::anyhow!("🔐 Permission denied\n\n💡 Try:\n  • Check Docker: docker ps\n  • Verify Docker permissions\n  • Restart Docker daemon")
            } else if (error_chain.contains("Docker") || error_str.contains("Docker daemon"))
                && !error_str.contains("No such container")
                && !error_str.contains("No such object") {
                anyhow::anyhow!("🐳 Docker issue detected\n\n💡 Try:\n  • Start Docker\n  • Check Docker: docker version\n  • Restart Docker daemon")
            } else if error_chain.contains("config") || error_str.contains("configuration") {
                anyhow::anyhow!("⚙️ Configuration issue\n\n💡 Try:\n  • Validate config: vm validate\n  • Check vm.yaml syntax\n  • Reset config: vm init")
            } else {
                e
            }
        }
    })
}
