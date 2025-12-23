// Command handlers for VM operations

use crate::error::{VmError, VmResult};
use tracing::debug;
// Import the CLI types
use crate::cli::{Args, BaseSubcommand, Command, PluginSubcommand, TunnelSubcommand};
use std::path::Path;
use vm_cli::msg;
use vm_config::{
    config::{PortsConfig, ProjectConfig, VmConfig},
    detector::detect_project_name,
    resources::detect_resource_defaults,
    AppConfig,
};
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::get_provider;

// Individual command modules
pub mod clean;
pub mod config;
pub mod db;
pub mod doctor;
pub mod env;
pub mod init;
pub mod plugin;
pub mod plugin_new;
pub mod registry;
pub mod secrets;
pub mod snapshot;
pub mod temp;
pub mod tunnel;
pub mod uninstall;
pub mod up;
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
        Command::Doctor { fix } => {
            debug!("Handling doctor command with fix={}", fix);
            doctor::run_with_fix(*fix).map_err(VmError::from)
        }
        Command::Up { command } => {
            debug!("Handling up command");
            up::handle_up(args.config.clone(), command.clone()).await
        }
        Command::Clean { dry_run, verbose } => {
            debug!("Handling clean command");
            clean::handle_clean(*dry_run, *verbose).await
        }
        Command::Init {
            file,
            services,
            ports,
            preset,
        } => {
            debug!("Handling init command");
            init::handle_init(file.clone(), services.clone(), *ports, preset.clone())
                .map_err(VmError::from)
        }
        Command::Config { command } => {
            debug!("Calling ConfigOps methods directly");
            config::handle_config_command(command, args.dry_run)
        }
        Command::Temp { command } => {
            debug!("Calling temp VM operations directly");
            temp::handle_temp_command(command, args.config)
        }
        Command::Registry { command } => {
            debug!("Calling registry operations");
            // For registry commands, use default GlobalConfig if no config file exists
            let global_config = match AppConfig::load(args.config.clone()) {
                Ok(app_config) => app_config.global,
                Err(_) => {
                    // Use default GlobalConfig when no config file exists
                    // This allows registry commands to work without a vm.yaml
                    vm_config::GlobalConfig::default()
                }
            };
            registry::handle_registry_command(command, global_config).await
        }
        Command::Secrets { command } => {
            debug!("Calling secrets operations");
            // For secrets commands, use default GlobalConfig if no config file exists
            let global_config = match AppConfig::load(args.config.clone()) {
                Ok(app_config) => app_config.global,
                Err(_) => {
                    // Use default GlobalConfig when no config file exists
                    // This allows secrets commands to work without a vm.yaml
                    vm_config::GlobalConfig::default()
                }
            };
            secrets::handle_secrets_command(command, global_config).await
        }
        Command::Plugin { command } => {
            debug!("Calling plugin operations");
            handle_plugin_command(command)
        }
        Command::Db { command } => {
            debug!("Calling db operations");
            db::handle_db(command.clone()).await
        }
        Command::Snapshot { command } => {
            debug!("Calling snapshot operations");
            snapshot::handle_snapshot(command.clone(), args.config).await
        }
        Command::Base { command } => {
            debug!("Calling base snapshot operations");
            handle_base_command(command, args.config).await
        }
        Command::Env { command } => {
            debug!("Calling env operations");
            env::handle_env_command(command, args.config)
        }
        Command::Completion { shell } => {
            debug!("Generating shell completions for: {}", shell);
            handle_completion(shell)
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
        | Command::Apply { .. } => {
            vm_println!("{}", MESSAGES.vm.dry_run_header);
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm.dry_run_command,
                    command = format!("{:?}", args.command)
                )
            );
            if let Some(config) = &args.config {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.vm.dry_run_config,
                        config = config.display().to_string()
                    )
                );
            }
            vm_println!("{}", MESSAGES.vm.dry_run_complete);
            Ok(())
        }
        Command::Ssh {
            container, command, ..
        } => {
            let app_config = AppConfig::load(args.config.clone())?;
            let project_name = app_config
                .vm
                .project
                .and_then(|p| p.name)
                .unwrap_or_default();
            let target = container.as_deref().unwrap_or(&project_name);
            if let Some(cmd) = command {
                vm_println!("Dry run: Would execute command `{}` on {}", cmd, target);
            } else {
                vm_println!("Dry run: Would connect to {}", target);
            }
            Ok(())
        }
        Command::Exec {
            container, command, ..
        } => {
            let app_config = AppConfig::load(args.config.clone())?;
            let project_name = app_config
                .vm
                .project
                .and_then(|p| p.name)
                .unwrap_or_default();
            let target = container.as_deref().unwrap_or(&project_name);
            vm_println!(
                "Dry run: Would execute command `{}` on {}",
                command.join(" "),
                target
            );
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

    let app_config = match AppConfig::load(args.config.clone()) {
        Ok(config) => config,
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("No vm.yaml found") {
                if matches!(args.command, Command::Create { .. }) {
                    vm_println!("📝 No vm.yaml found, generating a default configuration...");

                    let resources = detect_resource_defaults();
                    let default_vm_config = VmConfig {
                        provider: Some("docker".to_string()),
                        project: Some(ProjectConfig {
                            name: Some(detect_project_name()?),
                            ..Default::default()
                        }),
                        vm: Some(vm_config::config::VmSettings {
                            memory: Some(vm_config::config::MemoryLimit::Limited(resources.memory)),
                            cpus: Some(vm_config::config::CpuLimit::Limited(resources.cpus)),
                            ..Default::default()
                        }),
                        ..Default::default()
                    };

                    let config_path = Path::new("vm.yaml");
                    default_vm_config.write_to_file(config_path)?;
                    vm_println!("✓ Generated vm.yaml");

                    // Reload the AppConfig
                    AppConfig::load(args.config)?
                } else {
                    vm_println!("{}", MESSAGES.config.not_found);
                    vm_println!("{}", MESSAGES.config.not_found_hint);
                    return Err(VmError::from(e));
                }
            } else {
                return Err(VmError::from(e));
            }
        }
    };
    // Extract VM config and global config
    let mut config = app_config.vm;
    let global_config = app_config.global;

    // For snapshot builds, modify config BEFORE creating provider to avoid container name conflicts
    if let Command::Create {
        save_as: Some(ref save_as),
        from_dockerfile: Some(ref dockerfile_path),
        ..
    } = args.command
    {
        use vm_core::vm_println;

        vm_println!("Building from Dockerfile: {}", dockerfile_path.display());

        // Override box configuration
        let mut vm_settings = config.vm.take().unwrap_or_default();
        vm_settings.r#box = Some(vm_config::config::BoxSpec::String(
            dockerfile_path.to_string_lossy().to_string(),
        ));
        config.vm = Some(vm_settings);

        // Use temporary project name to avoid conflicts with existing VMs
        let clean_name = save_as.strip_prefix('@').unwrap_or(save_as);
        let temp_project_name = format!("{}-build", clean_name);

        vm_println!(
            "Using temporary project name '{}' for snapshot build",
            temp_project_name
        );

        let mut project_config = config.project.take().unwrap_or_default();
        project_config.name = Some(temp_project_name);
        config.project = Some(project_config);

        // Snapshot builds should not expose project ports or services (avoid host conflicts)
        config.ports = PortsConfig::default();
        // Disable services for snapshot builds to keep base images lightweight
        config.services.clear();
    }

    debug!(
        "Loaded configuration: provider={:?}, project_name={:?}",
        config.provider,
        config.project.as_ref().and_then(|p| p.name.as_ref())
    );

    // Validate configuration before proceeding
    // We skip the port availability check for all commands except `create`
    // to avoid errors when a container is already running.
    let skip_port_check = !matches!(args.command, Command::Create { .. });
    let validation_errors = config.validate(skip_port_check);
    if !validation_errors.is_empty() {
        vm_error!("{}", MESSAGES.common.validation_failed);
        for error in &validation_errors {
            vm_println!("  ❌ {}", error);
        }
        vm_println!("{}", MESSAGES.common.validation_hint);
        return Err(VmError::validation(
            format!(
                "Configuration has {} validation error(s)",
                validation_errors.len()
            ),
            None::<String>,
        ));
    }

    // Get the appropriate provider (with potentially modified config for snapshot builds)
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
            save_as,
            from_dockerfile,
            preserve_services,
            refresh_packages,
        } => {
            vm_ops::handle_create(
                provider,
                config.clone(),
                global_config.clone(),
                force,
                instance,
                verbose,
                save_as,
                from_dockerfile,
                preserve_services,
                refresh_packages,
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
        Command::Apply { container } => {
            vm_ops::handle_apply(provider, container.as_deref(), config.clone())
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
            no_backup,
            all,
            provider: provider_filter,
            pattern,
            preserve_services,
        } => {
            vm_ops::handle_destroy_enhanced(
                provider,
                container.as_deref(),
                config,
                global_config.clone(),
                &force,
                &no_backup,
                &all,
                provider_filter.as_deref(),
                pattern.as_deref(),
                preserve_services,
            )
            .await
        }
        Command::Ssh {
            container,
            path,
            command,
            force_refresh,
            no_refresh,
        } => vm_ops::handle_ssh(
            provider,
            container.as_deref(),
            path,
            command.map(|c| vec!["/bin/bash".to_string(), "-c".to_string(), c]),
            config,
            force_refresh,
            no_refresh,
        ),
        Command::Status { container } => vm_ops::handle_status(
            provider,
            container.as_deref(),
            config,
            global_config.clone(),
        ),
        Command::Wait {
            container,
            service,
            timeout,
        } => vm_ops::handle_wait(
            provider,
            container.as_deref(),
            service.as_deref(),
            timeout,
            config,
            global_config.clone(),
        ),
        Command::Ports { container } => vm_ops::handle_ports(
            provider,
            container.as_deref(),
            config,
            global_config.clone(),
        ),
        Command::Tunnel { command } => match command {
            TunnelSubcommand::Create { mapping, container } => tunnel::handle_tunnel(
                provider,
                mapping.as_str(),
                container.as_deref(),
                config,
                global_config.clone(),
            ),
            TunnelSubcommand::List { container } => tunnel::handle_tunnel_list(
                provider,
                container.as_deref(),
                config,
                global_config.clone(),
            ),
            TunnelSubcommand::Stop {
                port,
                container,
                all,
            } => tunnel::handle_tunnel_stop(
                provider,
                port.as_ref().copied(),
                container.as_deref(),
                all,
                config,
                global_config.clone(),
            ),
        },
        Command::Exec { container, command } => {
            vm_ops::handle_exec(provider, container.as_deref(), command, config.clone())
        }
        Command::Logs {
            container,
            follow,
            tail,
            service,
        } => vm_ops::handle_logs(
            provider,
            container.as_deref(),
            config.clone(),
            follow,
            tail,
            service.as_deref(),
        ),
        Command::Copy {
            source,
            destination,
            all_vms,
        } => vm_ops::handle_copy(provider, &source, &destination, all_vms, config.clone()),
        cmd => {
            vm_error!(
                "Command {:?} should have been handled in earlier match statement",
                cmd
            );
            Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Command not handled"),
                format!("Command {cmd:?} not handled in match statement"),
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

/// Handle base snapshot commands (shorthand for vm snapshot with @prefix)
async fn handle_base_command(
    command: &BaseSubcommand,
    config_path: Option<std::path::PathBuf>,
) -> VmResult<()> {
    use crate::cli::SnapshotSubcommand;

    // Convert base commands to snapshot commands with @ prefix
    let snapshot_command = match command {
        BaseSubcommand::List => SnapshotSubcommand::List {
            project: None,
            r#type: Some("base".to_string()),
        },
        BaseSubcommand::Create {
            name,
            description,
            from_dockerfile,
            build_context,
            build_arg,
        } => SnapshotSubcommand::Create {
            name: format!("@{}", name),
            description: description.clone(),
            quiesce: false,
            project: None,
            from_dockerfile: from_dockerfile.clone(),
            build_context: build_context.clone(),
            build_arg: build_arg.clone(),
        },
        BaseSubcommand::Delete { name, force } => SnapshotSubcommand::Delete {
            name: format!("@{}", name),
            project: None,
            force: *force,
        },
        BaseSubcommand::Restore { name, force } => SnapshotSubcommand::Restore {
            name: format!("@{}", name),
            project: None,
            force: *force,
        },
    };

    snapshot::handle_snapshot(snapshot_command, config_path).await
}

fn handle_completion(shell: &str) -> VmResult<()> {
    use clap::CommandFactory;
    use clap_complete::{generate, shells};
    use std::io;

    let mut cmd = crate::cli::Args::command();

    match shell.to_lowercase().as_str() {
        "bash" => {
            generate(shells::Bash, &mut cmd, "vm", &mut io::stdout());
            Ok(())
        }
        "zsh" => {
            generate(shells::Zsh, &mut cmd, "vm", &mut io::stdout());
            Ok(())
        }
        "fish" => {
            generate(shells::Fish, &mut cmd, "vm", &mut io::stdout());
            Ok(())
        }
        "powershell" => {
            generate(shells::PowerShell, &mut cmd, "vm", &mut io::stdout());
            Ok(())
        }
        _ => {
            vm_error!(
                "Unsupported shell: {}. Supported shells: bash, zsh, fish, powershell",
                shell
            );
            Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Unsupported shell"),
                format!(
                    "Shell '{}' is not supported. Use: bash, zsh, fish, or powershell",
                    shell
                ),
            ))
        }
    }
}
