// Command handlers for VM operations

use crate::cli::{Args, Command, PluginSubcommand, SecretSubcommand, TunnelSubcommand};
use crate::error::{VmError, VmResult};
use command_context::{
    load_provider_context, load_runtime_context, load_runtime_subject, project_name,
};
use environment::resolve_environment;
use vm_config::AppConfig;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;

pub mod base;
pub mod clean;
mod command_context;
mod completion;
pub mod config;
pub mod db;
pub mod doctor;
mod environment;
mod maintenance;
pub mod plugin;
pub mod plugin_new;
pub mod registry;
mod run;
pub mod secrets;
mod state;
mod status;
mod system;
pub mod tunnel;
pub mod uninstall;
pub mod update;
pub mod vm_ops;

#[must_use = "command execution results should be handled"]
pub async fn execute_command(args: Args) -> VmResult<()> {
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
                clean::handle_clean(false, false).await?;
            }
            if prune_pnpm_store {
                let subject =
                    load_runtime_context(args.config, args.profile, None, container.as_deref())?;
                maintenance::prune_pnpm_store(subject.provider, Some(subject.target.as_str()))?;
            }
            doctor::run_with_fix(fix).map_err(VmError::from)
        }
        Command::Config { command } => {
            config::handle_config_command(&command, false, args.profile, args.config)
        }
        Command::Plugin { command } => handle_plugin_command(&command),
        Command::Db { command } => db::handle_db(command).await,
        Command::Fleet { command } => vm_ops::handle_fleet_command(&command, false).await,
        Command::Secret { command } => {
            handle_secret_command(&command, args.config, args.profile).await
        }
        Command::System { command } => system::handle(&command, args.config, args.profile).await,
        Command::InternalCompletion { shell } => completion::handle(&shell),
        Command::List { all, raw } => {
            if all {
                vm_ops::handle_list_enhanced(None, None, raw, None)
            } else {
                let (provider, config, _) = load_provider_context(args.config, args.profile, None)?;
                let project = project_name(&config);
                let default_name = provider.resolve_instance_name(None).ok();
                vm_ops::handle_list_enhanced(None, Some(project), raw, default_name.as_deref())
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
        } => {
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
            let subject = load_runtime_subject(args.config, args.profile, environment)?;
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
            command,
        } => {
            if command.is_empty() {
                return Err(VmError::validation(
                    "No command was provided",
                    Some("Use: vm exec [environment] -- <command>"),
                ));
            }
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
            source,
            destination,
        } => {
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
        Command::Stop { environment } => {
            let subject = load_runtime_subject(args.config, args.profile, environment)?;
            vm_ops::handle_stop(
                subject.provider,
                Some(subject.target.as_str()),
                subject.config,
                subject.global_config,
            )
            .await
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
        Command::Restart { environment } => {
            let subject = load_runtime_subject(args.config, args.profile, environment)?;
            vm_ops::handle_restart(
                subject.provider,
                Some(subject.target.as_str()),
                subject.config,
                subject.global_config,
            )
            .await
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
    vm_println!("{}", MESSAGES.vm.dry_run_header);
    vm_println!("  Command: {:?}", args.command);
    if let Some(config) = &args.config {
        vm_println!("  Config: {}", config.display());
    }
    vm_println!("{}", MESSAGES.vm.dry_run_complete);
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
