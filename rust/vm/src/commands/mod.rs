// Command handlers for VM operations

use crate::error::{VmError, VmResult};
use tracing::debug;
// Import the CLI types
use crate::cli::{Args, Command, PluginSubcommand};
use vm_config::{init_config_file, AppConfig};
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::get_provider;

// Individual command modules
pub mod auth;
pub mod config;
pub mod doctor;
pub mod pkg;
pub mod plugin;
pub mod plugin_new;
pub mod temp;
pub mod uninstall;
pub mod update;
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
        Command::Doctor => {
            let app_config = AppConfig::load(args.config.clone())?;
            doctor::handle_doctor_command(app_config.global).await
        }
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
            // For pkg commands, use default GlobalConfig if no config file exists
            let global_config = match AppConfig::load(args.config.clone()) {
                Ok(app_config) => app_config.global,
                Err(_) => {
                    // Use default GlobalConfig when no config file exists
                    // This allows pkg commands to work without a vm.yaml
                    vm_config::GlobalConfig::default()
                }
            };
            pkg::handle_pkg_command(command, global_config).await
        }
        Command::Auth { command } => {
            debug!("Calling auth proxy operations");
            // For auth commands, use default GlobalConfig if no config file exists
            let global_config = match AppConfig::load(args.config.clone()) {
                Ok(app_config) => app_config.global,
                Err(_) => {
                    // Use default GlobalConfig when no config file exists
                    // This allows auth commands to work without a vm.yaml
                    vm_config::GlobalConfig::default()
                }
            };
            auth::handle_auth_command(command, global_config).await
        }
        Command::Plugin { command } => {
            debug!("Calling plugin operations");
            handle_plugin_command(command)
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

    let app_config = {
        // For List command, try lenient loading first to avoid validation errors
        if matches!(args.command, Command::List { .. }) {
            match config::load_app_config_lenient(args.config.clone()) {
                Ok(config) => config,
                Err(_) => {
                    // If lenient loading fails, fall back to strict loading
                    match AppConfig::load(args.config) {
                        Ok(config) => config,
                        Err(e) => {
                            let error_str = e.to_string();
                            #[allow(clippy::excessive_nesting)]
                            if error_str.contains("No vm.yaml found") {
                                vm_println!("{}", MESSAGES.config_not_found);
                                vm_println!("{}", MESSAGES.config_not_found_hint);
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
            match AppConfig::load(args.config) {
                Ok(config) => config,
                Err(e) => {
                    let error_str = e.to_string();
                    #[allow(clippy::excessive_nesting)]
                    if error_str.contains("No vm.yaml found") {
                        vm_println!("{}", MESSAGES.config_not_found);
                        vm_println!("{}", MESSAGES.config_not_found_hint);
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

    // Extract VM config and global config
    let config = app_config.vm;
    let global_config = app_config.global;

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
        Command::Create {
            force,
            instance,
            verbose,
        } => {
            vm_ops::handle_create(
                provider,
                config.clone(),
                global_config.clone(),
                force,
                instance,
                verbose,
            )
            .await
        }
        Command::Start { container } => {
            vm_ops::handle_start(
                provider,
                container.as_deref(),
                config.clone(),
                global_config.clone(),
            )
            .await
        }
        Command::Stop { container } => {
            vm_ops::handle_stop(
                provider,
                container.as_deref(),
                config.clone(),
                global_config.clone(),
            )
            .await
        }
        Command::Restart { container } => {
            vm_ops::handle_restart(
                provider,
                container.as_deref(),
                config.clone(),
                global_config.clone(),
            )
            .await
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
        Command::Update { version, force } => {
            update::handle_update(version.as_deref(), force)?;
            Ok(())
        }
        Command::Uninstall { keep_config, yes } => {
            uninstall::handle_uninstall(keep_config, yes)?;
            Ok(())
        }
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
                global_config.clone(),
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
        Command::Status { container } => vm_ops::handle_status(
            provider,
            container.as_deref(),
            config,
            global_config.clone(),
        ),
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

fn handle_plugin_command(command: &PluginSubcommand) -> VmResult<()> {
    match command {
        PluginSubcommand::List => plugin::handle_plugin_list().map_err(VmError::from),
        PluginSubcommand::Info { plugin_name } => {
            plugin::handle_plugin_info(plugin_name).map_err(VmError::from)
        }
        PluginSubcommand::Install { source_path } => {
            plugin::handle_plugin_install(source_path).map_err(VmError::from)
        }
        PluginSubcommand::Remove { plugin_name } => {
            plugin::handle_plugin_remove(plugin_name).map_err(VmError::from)
        }
        PluginSubcommand::New {
            plugin_name,
            r#type,
        } => plugin_new::handle_plugin_new(plugin_name, r#type).map_err(VmError::from),
        PluginSubcommand::Validate { plugin_name } => {
            plugin::handle_plugin_validate(plugin_name).map_err(VmError::from)
        }
    }
}
