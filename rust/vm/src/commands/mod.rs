// Command handlers for VM operations

use crate::cli::{Args, Command, PluginSubcommand, SecretSubcommand, TunnelSubcommand};
use crate::error::{VmError, VmResult};
use environment::resolve_environment;
use vm_config::{config::VmConfig, AppConfig};
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;
use vm_provider::get_provider;

pub mod base;
pub mod clean;
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
                    resolve_environment(args.config.clone(), args.profile.clone(), container)?;
                let (provider, _, _) =
                    load_provider_context(args.config, subject.profile, subject.provider_override)?;
                maintenance::prune_pnpm_store(provider, subject.target.as_deref())?;
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
            let project = if all {
                None
            } else {
                Some(load_project_name(
                    args.config.clone(),
                    args.profile.clone(),
                )?)
            };
            vm_ops::handle_list_enhanced(None, project.as_deref(), raw)
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
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_start(
                provider,
                subject.target.as_deref(),
                config,
                global_config,
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
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, _) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            match command {
                Some(command) => vm_ops::handle_exec(
                    provider,
                    subject.target.as_deref(),
                    vec!["/bin/sh".to_string(), "-c".to_string(), command],
                    config,
                ),
                None => vm_ops::handle_ssh(provider, subject.target.as_deref(), path, config),
            }
        }
        Command::Exec {
            environment,
            command,
        } => {
            let (provider, config, _) = load_provider_context(args.config, args.profile, None)?;
            vm_ops::handle_exec(provider, Some(environment.as_str()), command, config)
        }
        Command::Logs {
            environment,
            follow,
            tail,
            service,
        } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, _) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_logs(
                provider,
                subject.target.as_deref(),
                config,
                follow,
                tail,
                service.as_deref(),
            )
        }
        Command::Copy {
            source,
            destination,
        } => {
            let (provider, config, _) = load_provider_context(args.config, args.profile, None)?;
            vm_ops::handle_copy(provider, &source, &destination, config)
        }
        Command::Stop { environment } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_stop(provider, subject.target.as_deref(), config, global_config).await
        }
        Command::Status { environment } => {
            let targeted = environment.is_some() || args.profile.is_some();
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            if !targeted {
                let project = load_project_name(args.config, subject.profile)?;
                return vm_ops::handle_list_enhanced(None, Some(project).as_deref(), false);
            }

            let (provider, _config, _) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            let report = provider
                .status(subject.target.as_deref())
                .map_err(VmError::from)?;
            status::display(&report);
            Ok(())
        }
        Command::Restart { environment } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_restart(provider, subject.target.as_deref(), config, global_config).await
        }
        Command::Remove { environment, force } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_destroy(
                provider,
                subject.target.as_deref(),
                config,
                global_config,
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

fn load_project_name(
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<String> {
    let app_config = AppConfig::load(config_path, profile, None)?;
    Ok(app_config
        .vm
        .project
        .as_ref()
        .and_then(|project| project.name.clone())
        .unwrap_or_else(|| "vm-project".to_string()))
}

fn load_provider_context(
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
    provider_override: Option<String>,
) -> VmResult<(
    Box<dyn vm_provider::Provider>,
    VmConfig,
    vm_config::GlobalConfig,
)> {
    let app_config = AppConfig::load(config_path, profile, provider_override)?;
    let config = app_config.vm;
    let global_config = app_config.global;
    let provider = get_provider(config.clone()).map_err(VmError::from)?;
    Ok((provider, config, global_config))
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
