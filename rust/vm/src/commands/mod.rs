// Command handlers for VM operations

use crate::cli::{Args, Command};
use crate::error::{VmError, VmResult};
use command_context::{load_provider_context, load_runtime_context, load_runtime_subject};
use std::path::PathBuf;
use vm_config::validation::{validate_config, ValidationMode};
use vm_config::AppConfig;

pub mod base;
pub mod clean;
mod command_context;
mod completion;
pub mod config;
pub mod db;
mod declarations;
pub mod doctor;
mod environment;
mod maintenance;
mod managed_guest;
mod packages;
pub mod plugin;
pub mod plugin_new;
mod project;
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
pub async fn execute_command(mut args: Args) -> VmResult<()> {
    if let Some(selector) = args.project.take() {
        args.config = Some(project::resolve(&selector)?);
    }
    command_context::ensure_controller_host(&args.command)?;

    match args.command {
        Command::Init { path } => project::init(path),
        Command::Create {
            name,
            provider,
            image,
            snapshot,
            cpu,
            memory,
            mount,
        } => {
            declarations::create(declarations::CreateRequest {
                name,
                provider,
                image,
                snapshot,
                cpu,
                memory,
                mounts: mount,
                config_path: args.config,
                profile: args.profile,
            })
            .await
        }
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
        Command::Plugins { command } => plugin::handle_command(&command),
        Command::Db { command } => db::handle_db(command, args.config, args.profile).await,
        Command::Secrets { command } => {
            secrets::handle_command(&command, args.config, args.profile).await
        }
        Command::System { command } => system::handle(&command, args.config, args.profile).await,
        Command::InternalCompletion { shell } => completion::handle(&shell),
        Command::List {
            all_projects,
            raw,
            json,
        } => {
            let project_selected = args.config.is_some()
                || vm_config::ConfigLoader::new()
                    .find_config_path()
                    .map_err(VmError::from)?
                    .as_deref()
                    .is_some_and(|path| path.file_name().is_some_and(|name| name == "vm.yaml"));
            if all_projects || !project_selected {
                if json {
                    let instances = vm_ops::collect_list_instances(None, None, None)?;
                    crate::presentation::success("list", vm_ops::list_output(instances, None, raw))
                } else {
                    vm_ops::handle_list_enhanced(None, None, None, raw, None)
                }
            } else {
                let config = AppConfig::load(args.config.clone(), args.profile.clone(), None)?.vm;
                command_context::require_project_config(&config)?;
                if !config.environments.is_empty() {
                    return if json {
                        let (instances, default_name) =
                            vm_ops::collect_declared_project_instances(&config)?;
                        crate::presentation::success(
                            "list",
                            vm_ops::list_output(instances, default_name.as_deref(), raw),
                        )
                    } else {
                        vm_ops::handle_declared_project_list(&config, raw)
                    };
                }
                let (provider, config, _) = load_provider_context(args.config, args.profile, None)?;
                let default_name = provider.resolve_instance_name(None).ok();
                if json {
                    let instances = vm_ops::collect_list_instances(
                        Some(provider.as_ref()),
                        None,
                        Some(&config),
                    )?;
                    crate::presentation::success(
                        "list",
                        vm_ops::list_output(instances, default_name.as_deref(), raw),
                    )
                } else {
                    vm_ops::handle_list_enhanced(
                        Some(provider.as_ref()),
                        None,
                        Some(&config),
                        raw,
                        default_name.as_deref(),
                    )
                }
            }
        }
        Command::Start {
            environments,
            no_wait,
            fleet,
        } => {
            if fleet.fleet {
                let project = fleet_project(args.config, args.profile)?;
                vm_ops::handle_fleet_start(&fleet, &project, no_wait).await
            } else {
                let subjects = resolve_named_starts(args.config, args.profile, environments)?;
                let mut failures = Vec::new();
                for prepared in subjects {
                    let target = prepared.subject.target.clone();
                    if let Err(error) = start_prepared(prepared, no_wait).await {
                        failures.push(format!("{target}: {error}"));
                    }
                }
                named_outcome(failures)
            }
        }
        Command::Shell { environment, path } => {
            let subject = load_runtime_subject(args.config, args.profile, environment)?;
            vm_ops::handle_ssh(
                subject.provider,
                Some(subject.target.as_str()),
                path,
                subject.config,
            )
            .await
        }
        Command::Exec {
            environments,
            fleet,
            command,
        } => {
            if command.is_empty() {
                return Err(VmError::validation(
                    "No command was provided",
                    Some("Use: vm exec [--env NAME]... -- <command>"),
                ));
            }
            if fleet.fleet {
                let project = fleet_project(args.config, args.profile)?;
                vm_ops::handle_fleet_exec(&fleet, &project, &command)
            } else {
                let subjects = resolve_named_subjects(args.config, args.profile, environments)?;
                if subjects.len() == 1 {
                    let subject = subjects.into_iter().next().expect("one subject");
                    let exit = vm_ops::handle_exec(
                        subject.provider,
                        Some(subject.target.as_str()),
                        command,
                        subject.config,
                        subject.global_config,
                    )
                    .await?;
                    return if exit.success() {
                        Ok(())
                    } else {
                        Err(VmError::guest_exit(exit.code()))
                    };
                }
                let mut failures = Vec::new();
                for subject in subjects {
                    let target = subject.target.clone();
                    match vm_ops::handle_exec(
                        subject.provider,
                        Some(subject.target.as_str()),
                        command.clone(),
                        subject.config,
                        subject.global_config,
                    )
                    .await
                    {
                        Ok(exit) if !exit.success() => {
                            failures.push(format!(
                                "{target}: guest command exited with status {}",
                                exit.code()
                            ));
                        }
                        Err(error) => failures.push(format!("{target}: {error}")),
                        Ok(_) => {}
                    }
                }
                named_outcome(failures)
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
        Command::Stop {
            environments,
            fleet,
        } => {
            if fleet.fleet {
                let project = fleet_project(args.config, args.profile)?;
                vm_ops::handle_fleet_lifecycle(&fleet, &project, vm_ops::FleetAction::Stop, false)
                    .await
            } else {
                let subjects = resolve_named_subjects(args.config, args.profile, environments)?;
                let mut failures = Vec::new();
                for subject in subjects {
                    let target = subject.target.clone();
                    if let Err(error) = vm_ops::handle_stop(
                        subject.provider,
                        Some(subject.target.as_str()),
                        subject.config,
                        subject.global_config,
                    )
                    .await
                    {
                        failures.push(format!("{target}: {error}"));
                    }
                }
                named_outcome(failures)
            }
        }
        Command::Status {
            environments,
            fleet,
            json,
        } => {
            if json {
                return status_json(args.config, args.profile, environments, fleet);
            }
            if fleet.fleet {
                let project = fleet_project(args.config, args.profile)?;
                vm_ops::handle_fleet_status(&fleet, &project)
            } else {
                for subject in resolve_named_subjects(args.config, args.profile, environments)? {
                    let report = subject
                        .provider
                        .status(Some(subject.target.as_str()))
                        .map_err(VmError::from)?;
                    status::display(&report);
                    if !subject.provider.supports_runtime_drift_detection() {
                        vm_core::vm_println!(
                            "Configuration: unknown (provider has no drift record)"
                        );
                    } else {
                        match subject
                            .provider
                            .runtime_drift(&subject.target)
                            .map_err(VmError::from)?
                        {
                            Some(reason) => vm_core::vm_println!("Configuration drift: {reason}"),
                            None => vm_core::vm_println!("Configuration: current"),
                        }
                    }
                }
                Ok(())
            }
        }
        Command::Restart {
            environments,
            fleet,
        } => {
            if fleet.fleet {
                let project = fleet_project(args.config, args.profile)?;
                vm_ops::handle_fleet_lifecycle(
                    &fleet,
                    &project,
                    vm_ops::FleetAction::Restart,
                    false,
                )
                .await
            } else {
                let subjects = resolve_named_subjects(args.config, args.profile, environments)?;
                let mut failures = Vec::new();
                for subject in subjects {
                    let target = subject.target.clone();
                    if let Err(error) = vm_ops::handle_restart(
                        subject.provider,
                        Some(subject.target.as_str()),
                        subject.config,
                        subject.global_config,
                    )
                    .await
                    {
                        failures.push(format!("{target}: {error}"));
                    }
                }
                named_outcome(failures)
            }
        }
        Command::Remove {
            environments,
            fleet,
            delete_data,
            yes,
        } => {
            if fleet.fleet {
                let project = fleet_project(args.config, args.profile)?;
                vm_ops::handle_fleet_remove(&fleet, &project, delete_data, yes).await
            } else {
                let subjects = resolve_named_subjects(args.config, args.profile, environments)?;
                let names = subjects
                    .iter()
                    .map(|subject| format!("{} ({})", subject.target, subject.provider.name()))
                    .collect::<Vec<_>>();
                if !vm_ops::confirm_removal(&names, delete_data, yes)? {
                    return Ok(());
                }
                let mut failures = Vec::new();
                for subject in subjects {
                    let target = subject.target.clone();
                    if let Err(error) = vm_ops::handle_destroy(
                        subject.provider,
                        Some(subject.target.as_str()),
                        subject.config,
                        subject.global_config,
                        delete_data,
                    )
                    .await
                    {
                        failures.push(format!("{target}: {error}"));
                    }
                }
                named_outcome(failures)
            }
        }
        Command::Snapshots { command } => state::handle(command, args.config, args.profile).await,
        Command::Packages { command } => packages::handle(command, args.config, args.profile).await,
        Command::Tools { command } => tools::handle(command, args.config, args.profile).await,
        Command::Tunnels { command } => tunnel::handle_command(command, args.config, args.profile),
    }
}

fn fleet_project(
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<vm_ops::FleetProject> {
    let mut config = AppConfig::load(config_path, profile, None)?;
    packages::apply_client_environment(&mut config.vm)?;
    vm_ops::FleetProject::new(config.vm)
}

fn status_json(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environments: Vec<String>,
    fleet: crate::cli::FleetArgs,
) -> VmResult<()> {
    let (output, errors) = if fleet.fleet {
        let project = fleet_project(config_path, profile)?;
        vm_ops::collect_fleet_status(&fleet, &project)?
    } else {
        let subjects = resolve_named_subjects(config_path, profile, environments)?;
        let mut targets = Vec::with_capacity(subjects.len());
        let mut errors = Vec::new();
        for subject in subjects {
            let provider_name = subject.provider.name().to_string();
            let result = status::collect(subject.provider.as_ref(), &subject.target);
            match result {
                Ok(report) => targets.push(status::StatusTarget::success(
                    subject.target,
                    provider_name,
                    report,
                )),
                Err(error) => {
                    targets.push(status::StatusTarget::failure(
                        subject.target,
                        provider_name,
                        &error,
                    ));
                    errors.push(crate::presentation::ErrorRecord::from_error(&error));
                }
            }
        }
        (status::StatusOutput::from_targets(targets), errors)
    };
    let failed = output.failed;
    crate::presentation::outcome("status", output, errors)?;
    if failed > 0 {
        return Err(VmError::general(
            std::io::Error::new(
                std::io::ErrorKind::Other,
                "one or more status checks failed",
            ),
            format!("{failed} status checks failed"),
        )
        .reported());
    }
    Ok(())
}

fn resolve_named_subjects(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environments: Vec<String>,
) -> VmResult<Vec<command_context::RuntimeSubject>> {
    let names = if environments.is_empty() {
        vec![None]
    } else {
        environments.into_iter().map(Some).collect()
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut subjects = Vec::new();
    for name in names {
        let subject = load_runtime_subject(config_path.clone(), profile.clone(), name)?;
        if !seen.insert(subject.target.clone()) {
            return Err(VmError::validation(
                format!(
                    "Environment '{}' was selected more than once",
                    subject.target
                ),
                None::<String>,
            ));
        }
        subjects.push(subject);
    }
    Ok(subjects)
}

fn resolve_named_starts(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environments: Vec<String>,
) -> VmResult<Vec<command_context::PreparedStart>> {
    let names = if environments.is_empty() {
        vec![None]
    } else {
        environments.into_iter().map(Some).collect()
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut prepared = Vec::new();
    for name in names {
        let start = command_context::prepare_start(config_path.clone(), profile.clone(), name)?;
        if !seen.insert(start.subject.target.clone()) {
            return Err(VmError::validation(
                format!(
                    "Environment '{}' was selected more than once",
                    start.subject.target
                ),
                None::<String>,
            ));
        }
        prepared.push(start);
    }
    Ok(prepared)
}

async fn start_prepared(prepared: command_context::PreparedStart, no_wait: bool) -> VmResult<()> {
    let subject = prepared.subject;
    if let Some(name) = prepared.create_name {
        vm_ops::handle_create(
            subject.provider.clone_box(),
            subject.config.clone(),
            subject.global_config.clone(),
            false,
            Some(name),
        )
        .await?;
    }
    vm_ops::handle_start(
        subject.provider,
        Some(subject.target.as_str()),
        subject.config,
        subject.global_config,
        no_wait,
    )
    .await
}

fn named_outcome(failures: Vec<String>) -> VmResult<()> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("Environment operations failed:\n{}", failures.join("\n")),
            None::<String>,
        ))
    }
}
