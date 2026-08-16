// Command handlers for VM operations

use crate::cli::{Args, Command, PluginSubcommand, SecretSubcommand, TunnelSubcommand};
use crate::error::{VmError, VmResult};
use command_context::{
    load_or_create_runtime_subject, load_provider_context, load_runtime_context,
    load_runtime_subject, project_name,
};
use environment::resolve_environment;
use vm_config::validation::{validate_config, ValidationMode};
use vm_config::AppConfig;
use vm_core::vm_println;

pub mod base;
pub mod clean;
mod command_context;
mod completion;
pub mod config;
pub mod db;
pub mod doctor;
mod environment;
mod maintenance;
mod managed_guest;
mod packages;
pub mod plugin;
pub mod plugin_new;
pub(crate) mod remote_command;
mod run;
pub mod secrets;
mod state;
mod status;
mod system;
mod tools;
pub mod tunnel;
pub mod uninstall;
pub mod update;
pub mod vm_ops;

#[must_use = "command execution results should be handled"]
pub async fn execute_command(args: Args) -> VmResult<()> {
    command_context::ensure_controller_host(&args.command)?;

    if args.dry_run {
        print_dry_run_summary(&args);
        return Ok(());
    }

    match args.command {
        Command::Doctor {
            fix,
            clean,
            prune_pnpm_store,
            container,
        } => {
            if clean {
                clean::handle_clean().await?;
            }
            if prune_pnpm_store {
                let subject = load_runtime_context(
                    args.config.clone(),
                    args.profile.clone(),
                    None,
                    container.as_deref(),
                )?;
                maintenance::prune_pnpm_store(subject.provider, Some(subject.target.as_str()))?;
            }
            let loaded = AppConfig::load(args.config, args.profile, None);
            let provider = loaded
                .as_ref()
                .ok()
                .and_then(|config| config.vm.provider.clone())
                .unwrap_or_else(|| "docker".to_string());
            let configuration_error = match loaded {
                Ok(config) => match validate_config(&config.vm, ValidationMode::Static) {
                    Ok(report) if report.has_errors() => Some(report.to_string()),
                    Ok(_) => None,
                    Err(error) => Some(error.to_string()),
                },
                Err(error) => Some(error.to_string()),
            };
            doctor::run_with_fix(fix, &provider, configuration_error.as_deref())
                .map_err(VmError::from)
        }
        Command::Config { command } => {
            config::handle_config_command(&command, args.profile, args.config)
        }
        Command::Plugin { command } => handle_plugin_command(&command),
        Command::Db { command } => db::handle_db(command).await,
        Command::Secret { command } => {
            handle_secret_command(&command, args.config, args.profile).await
        }
        Command::System { command } => system::handle(&command, args.config, args.profile).await,
        Command::InternalCompletion { shell } => completion::handle(&shell),
        Command::List { all, raw } => {
            if all {
                vm_ops::handle_list_enhanced(None, None, None, raw, None)
            } else {
                let (provider, config, _) = load_provider_context(args.config, args.profile, None)?;
                let project = project_name(&config);
                let default_name = provider.resolve_instance_name(None).ok();
                vm_ops::handle_list_enhanced(
                    Some(provider.as_ref()),
                    None,
                    Some(project),
                    raw,
                    default_name.as_deref(),
                )
            }
        }
        Command::Create { environment, force } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_create(provider, config, global_config, force, subject.target).await
        }
        Command::Start {
            environment,
            no_wait,
            fleet,
        } => {
            if fleet.fleet {
                vm_ops::handle_fleet_lifecycle(&fleet, vm_ops::FleetAction::Start, no_wait).await
            } else {
                let subject = load_runtime_subject(args.config, args.profile, environment)?;
                vm_ops::handle_start(
                    subject.provider,
                    Some(subject.target.as_str()),
                    subject.config,
                    subject.global_config,
                    no_wait,
                )
                .await
            }
        }
        Command::Run {
            kind,
            words,
            provider,
            image,
            build,
            from_snapshot,
            ephemeral,
            mount,
            cpu,
            memory,
        } => {
            let name = run::parse_name(&words)?;
            run::handle(run::RunIntent {
                kind,
                name,
                provider_override: provider,
                image,
                build,
                from_snapshot,
                ephemeral,
                mounts: mount,
                cpu,
                memory,
                config_path: args.config,
                profile: args.profile,
            })
            .await
        }
        Command::Shell {
            environment,
            path,
            command,
        } => {
            let subject =
                load_or_create_runtime_subject(args.config, args.profile, environment).await?;
            match command {
                Some(command) => {
                    vm_ops::handle_exec(
                        subject.provider,
                        Some(subject.target.as_str()),
                        vec!["/bin/sh".to_string(), "-c".to_string(), command],
                        subject.config,
                        subject.global_config,
                    )
                    .await
                }
                None => {
                    vm_ops::handle_ssh(
                        subject.provider,
                        Some(subject.target.as_str()),
                        path,
                        subject.config,
                        subject.global_config,
                    )
                    .await
                }
            }
        }
        Command::Exec {
            environment,
            fleet,
            command,
        } => {
            if command.is_empty() {
                return Err(VmError::validation(
                    "No command was provided",
                    Some("Use: vm exec [environment] -- <command>"),
                ));
            }
            if fleet.fleet {
                vm_ops::handle_fleet_exec(&fleet, &command)
            } else {
                let subject = load_runtime_subject(args.config, args.profile, environment)?;
                vm_ops::handle_exec(
                    subject.provider,
                    Some(subject.target.as_str()),
                    command,
                    subject.config,
                    subject.global_config,
                )
                .await
            }
        }
        Command::Logs {
            environment,
            follow,
            tail,
            service,
        } => {
            let subject = load_runtime_subject(args.config, args.profile, environment)?;
            vm_ops::handle_logs(
                subject.provider,
                Some(subject.target.as_str()),
                subject.config,
                follow,
                tail,
                service.as_deref(),
            )
        }
        Command::Copy {
            fleet,
            source,
            destination,
        } => {
            if fleet.fleet {
                vm_ops::handle_fleet_copy(&fleet, &source, &destination)
            } else {
                let requested = vm_ops::target::copy_target(&source, &destination)?;
                let subject =
                    load_runtime_context(args.config, args.profile, None, requested.as_deref())?;
                vm_ops::handle_copy(
                    subject.provider,
                    &source,
                    &destination,
                    Some(subject.target.as_str()),
                    subject.config,
                )
            }
        }
        Command::Stop { environment, fleet } => {
            if fleet.fleet {
                vm_ops::handle_fleet_lifecycle(&fleet, vm_ops::FleetAction::Stop, false).await
            } else {
                let subject = load_runtime_subject(args.config, args.profile, environment)?;
                vm_ops::handle_stop(
                    subject.provider,
                    Some(subject.target.as_str()),
                    subject.config,
                    subject.global_config,
                )
                .await
            }
        }
        Command::Status { environment } => {
            let subject = load_runtime_subject(args.config, args.profile, environment)?;
            let report = subject
                .provider
                .status(Some(subject.target.as_str()))
                .map_err(VmError::from)?;
            status::display(&report);
            Ok(())
        }
        Command::Restart { environment, fleet } => {
            if fleet.fleet {
                vm_ops::handle_fleet_lifecycle(&fleet, vm_ops::FleetAction::Restart, false).await
            } else {
                let subject = load_runtime_subject(args.config, args.profile, environment)?;
                vm_ops::handle_restart(
                    subject.provider,
                    Some(subject.target.as_str()),
                    subject.config,
                    subject.global_config,
                )
                .await
            }
        }
        Command::Remove { environment, force } => {
            let subject = load_runtime_subject(args.config, args.profile, environment)?;
            vm_ops::handle_destroy(
                subject.provider,
                Some(subject.target.as_str()),
                subject.config,
                subject.global_config,
                force,
            )
            .await
        }
        Command::Save {
            words,
            description,
            quiesce,
            force,
        } => {
            let (environment, snapshot) = state::parse_save(&words)?;
            state::save(
                args.config,
                args.profile,
                environment,
                snapshot,
                description,
                quiesce,
                force,
            )
            .await
        }
        Command::Revert { words, force } => {
            let (environment, snapshot) = state::parse_revert(&words)?;
            state::revert(args.config, args.profile, environment, snapshot, force).await
        }
        Command::Package {
            environment,
            output,
            compress,
            build,
        } => {
            state::package(
                args.config,
                args.profile,
                environment,
                output,
                compress,
                build,
            )
            .await
        }
        Command::Packages { command } => packages::handle(command, args.config, args.profile).await,
        Command::Tools { command } => tools::handle(command, args.config, args.profile).await,
        Command::Tunnel { command } => handle_tunnel_command(command, args.config, args.profile),
        Command::GetSyncDirectory => {
            let (provider, _, _) = load_provider_context(args.config, args.profile, None)?;
            vm_ops::handle_get_sync_directory(provider);
            Ok(())
        }
    }
}

fn handle_tunnel_command(
    command: TunnelSubcommand,
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let (provider, config, global_config) = load_provider_context(config_path, profile, None)?;
    match command {
        TunnelSubcommand::Add {
            mapping,
            environment,
        } => tunnel::handle_tunnel(
            provider,
            &mapping,
            environment.as_deref(),
            config,
            global_config,
        ),
        TunnelSubcommand::Ls { environment } => {
            tunnel::handle_tunnel_list(provider, environment.as_deref(), config, global_config)
        }
        TunnelSubcommand::Stop {
            port,
            environment,
            all,
        } => tunnel::handle_tunnel_stop(
            provider,
            port,
            environment.as_deref(),
            all,
            config,
            global_config,
        ),
    }
}

async fn handle_secret_command(
    command: &SecretSubcommand,
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let global_config = AppConfig::load(config_path, profile, None)
        .map(|config| config.global)
        .unwrap_or_default();
    secrets::handle_secrets_command(command, global_config).await
}

fn print_dry_run_summary(args: &Args) {
    vm_println!("Dry run: would {}", dry_run_description(&args.command));
    if let Some(config) = &args.config {
        vm_println!("Config: {}", config.display());
    }
    vm_println!("No changes made.");
}

fn dry_run_description(command: &Command) -> String {
    let target = |environment: &Option<String>| {
        environment
            .as_deref()
            .unwrap_or("the default environment")
            .to_string()
    };

    match command {
        Command::Create { environment, force } => format!(
            "{} {}",
            if *force { "recreate" } else { "create" },
            target(environment)
        ),
        Command::Start {
            environment, fleet, ..
        } => {
            if fleet.fleet {
                "start matching managed environments".to_string()
            } else {
                format!("start {}", target(environment))
            }
        }
        Command::Run { kind, words, .. } => {
            let name = words
                .last()
                .filter(|_| words.len() == 2)
                .map_or(String::new(), |name| format!(" as {name}"));
            format!("run {}{name}", format!("{kind:?}").to_ascii_lowercase())
        }
        Command::List { all, .. } => {
            format!("list {} environments", if *all { "all" } else { "project" })
        }
        Command::Shell {
            environment,
            command,
            ..
        } => format!(
            "{} {}",
            if command.is_some() {
                "run a shell command in"
            } else {
                "open a shell in"
            },
            target(environment)
        ),
        Command::Exec {
            environment, fleet, ..
        } => {
            if fleet.fleet {
                "execute a command in matching managed environments".to_string()
            } else {
                format!("execute a command in {}", target(environment))
            }
        }
        Command::Logs { environment, .. } => format!("read logs from {}", target(environment)),
        Command::Copy { fleet, .. } => {
            if fleet.fleet {
                "copy files across matching managed environments".to_string()
            } else {
                "copy files without changing lifecycle state".to_string()
            }
        }
        Command::Stop {
            environment, fleet, ..
        } => {
            if fleet.fleet {
                "stop matching managed environments".to_string()
            } else {
                format!("stop {}", target(environment))
            }
        }
        Command::Status { environment } => format!("inspect {}", target(environment)),
        Command::Restart {
            environment, fleet, ..
        } => {
            if fleet.fleet {
                "restart matching managed environments".to_string()
            } else {
                format!("restart {}", target(environment))
            }
        }
        Command::Remove { environment, .. } => format!("remove {}", target(environment)),
        Command::Save { .. } => "save an environment snapshot".to_string(),
        Command::Revert { .. } => "restore an environment snapshot".to_string(),
        Command::Package { environment, .. } => format!("package {}", target(environment)),
        Command::Packages { .. } => "manage package infrastructure".to_string(),
        Command::Tools { .. } => "manage guest tools".to_string(),
        Command::Config { .. } => "run a configuration operation".to_string(),
        Command::Tunnel { .. } => "run a tunnel operation".to_string(),
        Command::Doctor { .. } => "run diagnostics or requested maintenance".to_string(),
        Command::Plugin { .. } => "run a plugin operation".to_string(),
        Command::System { .. } => "run a system operation".to_string(),
        Command::Db { .. } => "run a database operation".to_string(),
        Command::Secret { .. } => "run a secret operation (values redacted)".to_string(),
        Command::InternalCompletion { .. } => "generate shell completions".to_string(),
        Command::GetSyncDirectory => "print the workspace directory".to_string(),
    }
}

fn handle_plugin_command(command: &PluginSubcommand) -> VmResult<()> {
    match command {
        PluginSubcommand::Ls => plugin::handle_plugin_list().map_err(VmError::from),
        PluginSubcommand::Info { plugin_name } => {
            plugin::handle_plugin_info(plugin_name).map_err(VmError::from)
        }
        PluginSubcommand::Install { source_path } => {
            plugin::handle_plugin_install(source_path).map_err(VmError::from)
        }
        PluginSubcommand::Rm { plugin_name } => {
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
