// Command handlers for VM operations

use crate::error::{VmError, VmResult};
use tracing::debug;
// Import the CLI types
use crate::cli::{Args, Command};
use vm_common::{vm_error, vm_println};
use vm_config::{config::VmConfig, init_config_file};
use vm_provider::get_provider;

// Individual command modules
pub mod auth;
pub mod config;
pub mod doctor;
pub mod pkg;
pub mod registry;
pub mod temp;
pub mod vm_ops;

/// Main command dispatcher
#[must_use = "command execution results should be handled"]
pub async fn execute_command(args: Args) -> VmResult<()> {
    // Handle dry-run for provider commands
    if args.dry_run {
        return handle_dry_run(&args).await;
    }

    // Handle commands that don't need a provider first
    match &args.command {
        Command::Validate => config::handle_validate(args.config),
        Command::Doctor => doctor::handle_doctor_command().await,
        Command::Init {
            file,
            services,
            ports,
        } => {
            debug!("Calling init_config_file directly");
            init_config_file(file.clone(), services.clone(), *ports).map_err(VmError::from)
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
            pkg::handle_pkg_command(command, args.config).await
        }
        Command::Auth { command } => {
            debug!("Calling auth proxy operations");
            auth::handle_auth_command(command, args.config).await
        }
        Command::Registry { command } => {
            debug!("Calling Docker registry operations");
            registry::handle_registry_command(command, args.config).await
        }
        _ => {
            // Provider-based commands
            handle_provider_command(args).await
        }
    }
}

async fn handle_dry_run(args: &Args) -> VmResult<()> {
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
            Box::pin(execute_command(args_copy)).await
        }
    }
}

async fn handle_provider_command(args: Args) -> VmResult<()> {
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
                                return Err(VmError::config(
                                    std::io::Error::new(
                                        std::io::ErrorKind::NotFound,
                                        "Configuration required",
                                    ),
                                    "No vm.yaml configuration file found",
                                ));
                            }
                            return Err(VmError::from(e));
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
                        return Err(VmError::config(
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "Configuration required",
                            ),
                            "No vm.yaml configuration file found",
                        ));
                    }
                    return Err(VmError::from(e));
                }
            }
        }
    };

    debug!(
        "Loaded configuration: provider={:?}, project_name={:?}",
        config.provider,
        config.project.as_ref().and_then(|p| p.name.as_ref())
    );

    // Validate configuration before proceeding
    let validation_errors = config.validate();
    if !validation_errors.is_empty() {
        vm_error!("Configuration validation failed:");
        for error in &validation_errors {
            vm_println!("  ❌ {}", error);
        }
        vm_println!("\n💡 Fix the configuration errors above or run 'vm doctor' for more details");
        return Err(VmError::validation(
            format!(
                "Configuration has {} validation error(s)",
                validation_errors.len()
            ),
            None::<String>,
        ));
    }

    // Get the appropriate provider
    let provider = get_provider(config.clone()).map_err(VmError::from)?;

    // Log provider being used
    debug!(provider = %provider.name(), "Using provider");

    // Execute the command with friendly error handling
    debug!("Executing command: {:?}", args.command);
    let result = match args.command {
        Command::Create { force, instance } => {
            vm_ops::handle_create(provider, config.clone(), force, instance).await
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
        } => {
            vm_ops::handle_destroy_enhanced(
                provider,
                container.as_deref(),
                config,
                &force,
                &all,
                provider_filter.as_deref(),
                pattern.as_deref(),
            )
            .await
        }
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
            Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Command not handled"),
                format!("Command {:?} not handled in match statement", cmd),
            ))
        }
    };

    result
}
