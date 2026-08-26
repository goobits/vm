// Command handlers for VM operations

use crate::cli::{Args, Command};
use crate::error::{VmError, VmResult};
use command_context::{
    load_or_create_runtime_subject, load_provider_context, load_runtime_context,
    load_runtime_subject, project_name,
};
use environment::resolve_environment;
use vm_config::validation::{validate_config, ValidationMode};
use vm_config::AppConfig;
use vm_core::vm_warning;

pub mod base;
pub mod clean;
mod command_context;
mod completion;
pub mod config;
pub mod db;
pub mod doctor;
mod dry_run;
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
        dry_run::print_summary(&args);
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
                .map_or_else(|| "docker".to_string(), |provider| provider.to_string());
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
        Command::Plugin { command } => plugin::handle_command(&command),
        Command::Db { command } => db::handle_db(command).await,
        Command::Secret { command } => {
            secrets::handle_command(&command, args.config, args.profile).await
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
            vm_warning!(
                "`vm create` is deprecated; use `vm run <mac|linux|container>` or `vm shell` before v6.0.0"
            );
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
        Command::Import {
            archive,
            name,
            force,
        } => state::import(archive, name, force).await,
        Command::Packages { command } => packages::handle(command, args.config, args.profile).await,
        Command::Tools { command } => tools::handle(command, args.config, args.profile).await,
        Command::Tunnel { command } => tunnel::handle_command(command, args.config, args.profile),
        Command::GetSyncDirectory => {
            vm_warning!(
                "`vm get-sync-directory` is deprecated and will be removed in v6.0.0; use the configured provider workspace path"
            );
            let (provider, _, _) = load_provider_context(args.config, args.profile, None)?;
            vm_ops::handle_get_sync_directory(provider);
            Ok(())
        }
    }
}
