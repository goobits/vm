mod appliance;
mod catalog;
mod checkout;
mod consumer;
mod discovery;
mod docker;
mod files;
mod integration;
mod overrides;
mod process;
mod release;
mod runtime;
mod submission;
mod tart;
pub(in crate::commands) mod tooling;

use std::path::PathBuf;

use crate::cli::PackagesSubcommand;
use crate::commands::command_context::managed_guest_context;
use crate::error::{VmError, VmResult};
use vm_core::{vm_println, vm_success};

use appliance::configured_client;
use files::ApplianceFiles;
pub(super) use runtime::{apply_client_environment, reconcile_client_settings};

pub(in crate::commands) fn git_auth_configured() -> VmResult<bool> {
    ApplianceFiles::discover()?.has_git_token()
}

pub(in crate::commands) fn diagnose_client_access(fix: bool) -> VmResult<Option<bool>> {
    let files = ApplianceFiles::discover()?;
    let Some(mut state) = files.read_state()? else {
        return Ok(None);
    };
    if fix {
        state = appliance::repair_client_access(&files, state)?;
    }
    Ok(Some(appliance::state_client_access_is_current(
        &files, &state,
    )?))
}

async fn status(
    files: &ApplianceFiles,
    runtime: crate::cli::PackageInfrastructureRuntime,
) -> VmResult<()> {
    let global = vm_config::GlobalConfig::load()?;
    let source_state = catalog::prepare_source_roots(&global.packages.source_roots);
    let mut health = appliance::status(files, runtime).await?;
    if source_state.is_err()
        || (!global.packages.source_roots.is_empty() && !files.has_git_token()?)
    {
        health = appliance::PackageHealth::ActionRequired;
    } else if health == appliance::PackageHealth::Healthy
        && catalog::has_quarantined_sources(&global.packages.source_roots)
    {
        health = appliance::PackageHealth::Degraded;
    }
    vm_println!("Package infrastructure: {}", health.label());
    Ok(())
}

async fn doctor(
    files: &ApplianceFiles,
    runtime: crate::cli::PackageInfrastructureRuntime,
    fix: bool,
) -> VmResult<()> {
    let global = vm_config::GlobalConfig::load()?;
    if fix {
        if let Some(state) = files.read_state()? {
            appliance::repair_client_access(files, state)?;
        }
        let _ = catalog::repair_github_credential(files)?;
    }
    appliance::doctor(files, runtime).await?;

    let mut unresolved = Vec::new();
    if fix && files.read_state()?.is_some() {
        let repaired =
            catalog::repair_quarantined_sources(files, &global.packages.source_roots).await?;
        unresolved.extend(repaired.failures);
        let plan = catalog::prepare_source_roots(&global.packages.source_roots)?;
        let reconciled = catalog::reconcile_source_roots(files, plan).await?;
        unresolved.extend(reconciled.failures);
    } else {
        let plan = catalog::prepare_source_roots(&global.packages.source_roots)?;
        unresolved.extend(
            plan.discovery
                .failures
                .iter()
                .map(|failure| failure.message.clone()),
        );
    }

    for failure in &unresolved {
        vm_core::vm_warning!("{failure}");
    }
    let health = if files.read_state()?.is_none()
        || (!global.packages.source_roots.is_empty() && !files.has_git_token()?)
    {
        appliance::PackageHealth::ActionRequired
    } else if !unresolved.is_empty()
        || catalog::has_quarantined_sources(&global.packages.source_roots)
    {
        appliance::PackageHealth::Degraded
    } else {
        appliance::PackageHealth::Healthy
    };
    vm_println!("Package infrastructure: {}", health.label());
    if health == appliance::PackageHealth::ActionRequired {
        return Err(crate::error::VmError::validation(
            "Package infrastructure requires an operator action",
            Some("Run `vm packages up`"),
        ));
    }
    Ok(())
}

async fn up(
    files: &ApplianceFiles,
    runtime: crate::cli::PackageInfrastructureRuntime,
    port: u16,
    registry_image: Option<String>,
    job_image: Option<String>,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let global_config = vm_config::GlobalConfig::load()?;
    let source_roots = catalog::prepare_source_roots(&global_config.packages.source_roots)?;
    appliance::up(files, runtime, port, registry_image, job_image).await?;
    let outcome = catalog::reconcile_source_roots(files, source_roots).await?;
    if outcome.is_degraded() {
        vm_core::vm_warning!(
            "Package infrastructure is degraded: {} quarantined, {} unresolved",
            outcome.quarantined.len(),
            outcome.failures.len()
        );
        vm_core::vm_hint!("Repair with: vm packages doctor --fix");
        vm_println!("Package infrastructure: degraded");
    } else {
        vm_println!("Package infrastructure: healthy");
    }
    if let Ok(config) = vm_config::AppConfig::load(config_path, profile, None) {
        let _ = tooling::refresh(&config.vm).await;
    }
    Ok(())
}

fn package_init_context(
    source_root: PathBuf,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<(PathBuf, PathBuf, Option<String>)> {
    let config = match config_path {
        Some(path) => path,
        None => vm_config::config_ops::find_local_config().map_err(VmError::from)?,
    };
    let config = std::fs::canonicalize(&config).map_err(|error| {
        VmError::filesystem(
            error,
            config.display().to_string(),
            "resolve package work project configuration",
        )
    })?;
    let resolved =
        crate::commands::environment::resolve_environment(Some(config.clone()), profile, None)?;

    std::fs::create_dir_all(&source_root).map_err(|error| {
        VmError::filesystem(
            error,
            source_root.display().to_string(),
            "create package source root",
        )
    })?;
    let source_root = std::fs::canonicalize(&source_root).map_err(|error| {
        VmError::filesystem(
            error,
            source_root.display().to_string(),
            "resolve package source root",
        )
    })?;
    Ok((source_root, config, resolved.profile))
}

fn remember_package_init(
    source_root: &std::path::Path,
    config: &std::path::Path,
    profile: Option<String>,
) -> VmResult<()> {
    let mut global = vm_config::GlobalConfig::load()?;
    let source_root = source_root.to_string_lossy().into_owned();
    if !global.packages.source_roots.contains(&source_root) {
        global.packages.source_roots.push(source_root);
        global.packages.source_roots.sort();
    }
    global.packages.work_config = Some(config.to_string_lossy().into_owned());
    global.packages.work_profile = profile;
    global.save().map_err(VmError::from)
}

pub(super) async fn handle(
    command: PackagesSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    if managed_guest_context() {
        return handle_guest(command, config_path, profile).await;
    }
    let files = ApplianceFiles::discover()?;
    let _operation_lock = match &command {
        PackagesSubcommand::Backups { .. }
        | PackagesSubcommand::Backup { .. }
        | PackagesSubcommand::Restore { .. } => None,
        PackagesSubcommand::Init { .. }
        | PackagesSubcommand::Up { .. }
        | PackagesSubcommand::Down { .. } => Some(files.acquire_lifecycle_lock()?),
        _ => Some(files.acquire_operation_lock()?),
    };
    match command {
        PackagesSubcommand::Init { source_root } => {
            let (source_root, config, profile) =
                package_init_context(source_root, config_path, profile)?;
            remember_package_init(&source_root, &config, profile.clone())?;
            let _ = catalog::repair_github_credential(&files)?;
            up(
                &files,
                crate::cli::PackageInfrastructureRuntime::Auto,
                3080,
                None,
                None,
                Some(config),
                profile,
            )
            .await?;
            vm_success!("Package work is initialized");
            Ok(())
        }
        PackagesSubcommand::Up {
            runtime,
            port,
            registry_image,
            job_image,
        } => {
            up(
                &files,
                runtime,
                port,
                registry_image,
                job_image,
                config_path,
                profile,
            )
            .await
        }
        PackagesSubcommand::Down { runtime } => appliance::down(&files, runtime),
        PackagesSubcommand::Status { runtime } => status(&files, runtime).await,
        PackagesSubcommand::Doctor { runtime, fix } => doctor(&files, runtime, fix).await,
        PackagesSubcommand::Backups { runtime } => appliance::list_backups(&files, runtime),
        PackagesSubcommand::Backup { runtime } => appliance::backup(&files, runtime),
        PackagesSubcommand::Restore { backup_id, runtime } => {
            appliance::restore(&files, runtime, &backup_id)
        }
        PackagesSubcommand::Register {
            targets,
            ecosystem,
            repository,
            branch,
            recursive,
        } => {
            catalog::register(
                &files,
                catalog::RegistrationIntent {
                    targets,
                    ecosystem,
                    repository,
                    branch,
                    recursive,
                },
            )
            .await
        }
        PackagesSubcommand::List => catalog::list(&files).await,
        PackagesSubcommand::Consumer { command } => consumer::handle_catalog(&files, command).await,
        PackagesSubcommand::Consumers { package } => {
            consumer::show_consumers(&files, &package).await
        }
        PackagesSubcommand::Drift => consumer::show_drift(&files).await,
        PackagesSubcommand::Checkout {
            package,
            agent,
            consumer,
            task,
        } => {
            checkout::handle(
                &files,
                checkout::CheckoutIntent {
                    config_path,
                    profile,
                    package,
                    agent,
                    consumer,
                    task,
                },
            )
            .await
        }
        PackagesSubcommand::Show { checkout_id } => catalog::show(&files, &checkout_id).await,
        PackagesSubcommand::Release { .. } => Err(crate::error::VmError::validation(
            "Managed source releases run inside the assigned environment",
            Some("Run `vm packages release <checkout-id>` inside that Docker or Tart guest"),
        )),
        PackagesSubcommand::Cancel { checkout_id } => {
            cancel_checkout(&files, config_path, profile, &checkout_id).await
        }
        PackagesSubcommand::Cleanup { checkout_id } => {
            cleanup_checkout(&files, config_path, profile, &checkout_id).await
        }
        PackagesSubcommand::Auth {
            token_file,
            github,
            clear,
        } => catalog::configure_auth(&files, token_file, github, clear),
    }
}

async fn handle_guest(
    command: PackagesSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    match command {
        PackagesSubcommand::Init { .. } => Err(crate::error::VmError::validation(
            "Package initialization runs on the controller host",
            Some("Run on the host: vm packages init <source-root>"),
        )),
        PackagesSubcommand::Status { .. } => catalog::status_guest().await,
        PackagesSubcommand::Checkout {
            package,
            agent,
            consumer,
            task,
        } => {
            checkout::handle_guest(checkout::CheckoutIntent {
                config_path,
                profile,
                package,
                agent,
                consumer,
                task,
            })
            .await
        }
        PackagesSubcommand::Show { checkout_id } => catalog::show_guest(&checkout_id).await,
        PackagesSubcommand::Release { checkout_id } => release::handle_guest(&checkout_id).await,
        _ => Err(crate::error::VmError::validation(
            "This package command is restricted to the controller host",
            Some("Run package administration commands on the host"),
        )),
    }
}

async fn cleanup_checkout(
    files: &ApplianceFiles,
    config_path: Option<PathBuf>,
    profile: Option<String>,
    checkout_id: &str,
) -> VmResult<()> {
    let client = configured_client(files)?;
    let checkout = client.checkout(checkout_id).await?;
    checkout::cleanup_local(config_path, profile, &checkout).await?;
    let closed = client
        .cleanup_checkout(
            checkout_id,
            &vm_packages::CleanupRequest {
                actor: "vm-controller".into(),
                idempotency_key: format!("cleanup-{checkout_id}"),
            },
        )
        .await?;
    vm_success!("Checkout {} is {:?}", closed.checkout_id, closed.state);
    Ok(())
}

async fn cancel_checkout(
    files: &ApplianceFiles,
    config_path: Option<PathBuf>,
    profile: Option<String>,
    checkout_id: &str,
) -> VmResult<()> {
    configured_client(files)?
        .transition(
            checkout_id,
            &vm_packages::TransitionRequest {
                next: vm_packages::WorkflowState::Cancelled,
                actor: "vm-controller".into(),
                reason: "checkout cancelled by operator".into(),
                commit: None,
                validation_result: Some("cancelled".into()),
                idempotency_key: format!("cancel-{checkout_id}"),
            },
        )
        .await?;
    cleanup_checkout(files, config_path, profile, checkout_id).await
}
