mod access;
mod appliance;
mod catalog;
mod checkout;
mod client_settings;
mod consumer;
mod container;
mod credentials;
mod discovery;
mod files;
mod guest_checkout;
mod guest_runtime;
mod integration;
mod overrides;
mod process;
mod registration;
mod release;
mod source_images;
mod sources;
mod state;
mod submission;
pub(in crate::commands) mod tooling;
mod work_session;
mod workspace;

use std::path::PathBuf;

use crate::cli::{
    PackageAuthSubcommand, PackageBackupSubcommand, PackageServiceSubcommand, PackagesSubcommand,
};
use crate::commands::command_context::managed_guest_context;
use crate::error::{VmError, VmResult};
use vm_core::{vm_println, vm_success};

pub(super) use client_settings::{apply_client_environment, reconcile_client_settings};
use files::ApplianceFiles;

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

async fn status(files: &ApplianceFiles) -> VmResult<()> {
    let mut health = appliance::status(files)
        .await
        .unwrap_or(appliance::PackageHealth::ActionRequired);
    if let Ok(global) = vm_config::GlobalConfig::load() {
        if let Ok(plans) = sources::prepare_sources(files, &global.packages).await {
            if health == appliance::PackageHealth::Healthy
                && (sources::has_quarantined_sources(&global.packages.source_roots)
                    || plans.iter().any(|plan| plan.discovery.is_degraded()))
            {
                health = appliance::PackageHealth::Degraded;
            }
        } else {
            health = appliance::PackageHealth::ActionRequired;
        }
    } else {
        health = appliance::PackageHealth::ActionRequired;
    }
    vm_println!("Package infrastructure: {}", health.label());
    Ok(())
}

async fn doctor(files: &ApplianceFiles, fix: bool) -> VmResult<()> {
    let global = vm_config::GlobalConfig::load()?;
    if fix {
        if let Some(state) = files.read_state()? {
            appliance::repair_client_access(files, state)?;
        }
        let _ = credentials::repair_github(files)?;
        if files.read_state()?.is_some() {
            let _ = crate::commands::tools::activation::repair().await?;
        }
    }
    appliance::doctor(files).await?;

    let mut unresolved = Vec::new();
    if fix && files.read_state()?.is_some() {
        let repaired =
            sources::repair_quarantined_sources(files, &global.packages.source_roots).await?;
        unresolved.extend(repaired.failures);
        let plans = sources::prepare_sources(files, &global.packages).await?;
        let reconciled = sources::reconcile_source_plans(files, plans).await?;
        unresolved.extend(reconciled.failures);
    } else {
        for plan in sources::prepare_sources(files, &global.packages).await? {
            unresolved.extend(plan.discovery.diagnostics());
        }
    }

    for failure in &unresolved {
        vm_core::vm_warning!("{failure}");
    }
    let health = if files.read_state()?.is_none() {
        appliance::PackageHealth::ActionRequired
    } else if !unresolved.is_empty()
        || sources::has_quarantined_sources(&global.packages.source_roots)
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
    engine: crate::cli::PackageInfrastructureEngine,
    port: Option<u16>,
    registry_image: Option<String>,
    job_image: Option<String>,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let global_config = vm_config::GlobalConfig::load()?;
    let managed_sources = sources::prepare_source_roots(&global_config.packages.source_roots)?;
    appliance::up(files, engine, port, registry_image, job_image).await?;
    let source_plans =
        sources::prepare_sources_with_managed(files, &global_config.packages, managed_sources)
            .await?;
    let outcome = sources::reconcile_source_plans(files, source_plans).await?;
    if outcome.is_degraded() {
        vm_core::vm_warning!(
            "Package infrastructure is degraded: {} quarantined, {} unresolved",
            outcome.quarantined.len(),
            outcome.failures.len()
        );
        vm_core::vm_hint!("Repair with: vm packages service doctor --fix");
        vm_println!("Package infrastructure: degraded");
    } else {
        vm_println!("Package infrastructure: healthy");
    }
    if let Ok(config) = vm_config::AppConfig::load(config_path, profile, None) {
        let _ = tooling::refresh(&config.vm).await;
    }
    crate::commands::tools::activation::ensure_worker()?;
    Ok(())
}

fn prepare_source_root(source_root: PathBuf) -> VmResult<PathBuf> {
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
    Ok(source_root)
}

fn remember_source_root(source_root: &std::path::Path) -> VmResult<()> {
    let mut global = vm_config::GlobalConfig::load()?;
    let source_root = source_root.to_string_lossy().into_owned();
    if !global.packages.source_roots.contains(&source_root) {
        global.packages.source_roots.push(source_root);
        global.packages.source_roots.sort();
    }
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
        PackagesSubcommand::Service {
            command: PackageServiceSubcommand::Backups { .. },
        }
        | PackagesSubcommand::Open { .. } => None,
        PackagesSubcommand::Service {
            command: PackageServiceSubcommand::Init { .. },
        }
        | PackagesSubcommand::Up { .. }
        | PackagesSubcommand::Down => Some(files.acquire_lifecycle_lock()?),
        _ => Some(files.acquire_operation_lock()?),
    };
    match command {
        PackagesSubcommand::Service {
            command:
                PackageServiceSubcommand::Init {
                    source_root,
                    engine,
                    port,
                    registry_image,
                    job_image,
                },
        } => {
            let source_root = prepare_source_root(source_root)?;
            remember_source_root(&source_root)?;
            let _ = credentials::repair_github(&files)?;
            up(
                &files,
                engine,
                Some(port),
                registry_image,
                job_image,
                config_path,
                profile,
            )
            .await?;
            vm_success!("Package infrastructure is initialized");
            Ok(())
        }
        PackagesSubcommand::Up {
            engine,
            port,
            registry_image,
            job_image,
        } => {
            up(
                &files,
                engine,
                port,
                registry_image,
                job_image,
                config_path,
                profile,
            )
            .await
        }
        PackagesSubcommand::Down => appliance::down(&files),
        PackagesSubcommand::Service {
            command: PackageServiceSubcommand::Status,
        } => status(&files).await,
        PackagesSubcommand::Service {
            command: PackageServiceSubcommand::Doctor { fix },
        } => doctor(&files, fix).await,
        PackagesSubcommand::Service {
            command: PackageServiceSubcommand::Backups { command },
        } => match command {
            PackageBackupSubcommand::List => appliance::list_backups(&files),
            PackageBackupSubcommand::Create => appliance::backup(&files),
            PackageBackupSubcommand::Restore { name } => appliance::restore(&files, &name),
        },
        PackagesSubcommand::Register {
            targets,
            ecosystem,
            repository,
            branch,
            recursive,
        } => {
            registration::register(
                &files,
                registration::RegistrationIntent {
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
        PackagesSubcommand::Show { name } => catalog::show_package(&files, &name).await,
        PackagesSubcommand::Consumers { command } => {
            consumer::handle_catalog(&files, command).await
        }
        PackagesSubcommand::Open { source } => work_session::open(&files, source, profile).await,
        PackagesSubcommand::Checkout { source } => Err(VmError::validation(
            "Managed source checkout runs inside a managed VM",
            Some(format!(
                "Run inside a managed VM: vm packages checkout {source}"
            )),
        )),
        PackagesSubcommand::CheckoutShow { checkout_id } => {
            catalog::show(&files, &checkout_id).await
        }
        PackagesSubcommand::Release => Err(crate::error::VmError::validation(
            "Managed source releases run inside the assigned environment",
            Some("Run `vm packages release` from the source directory inside that managed VM"),
        )),
        PackagesSubcommand::Cancel => Err(crate::error::VmError::validation(
            "Managed checkout cancellation runs inside the assigned environment",
            Some("Run `vm packages cancel` from the managed checkout source directory"),
        )),
        PackagesSubcommand::Auth { command } => match command {
            PackageAuthSubcommand::Login {
                token_stdin,
                token_file,
            } => credentials::login(&files, token_stdin, token_file),
            PackageAuthSubcommand::Status => credentials::status(&files),
            PackageAuthSubcommand::Logout => credentials::logout(&files),
        },
    }
}

async fn handle_guest(
    command: PackagesSubcommand,
    _config_path: Option<PathBuf>,
    _profile: Option<String>,
) -> VmResult<()> {
    match command {
        PackagesSubcommand::Service {
            command: PackageServiceSubcommand::Init { .. },
        } => Err(crate::error::VmError::validation(
            "Package initialization runs on the controller host",
            Some("Run on the host: vm packages service init --source-root <path>"),
        )),
        PackagesSubcommand::Service {
            command: PackageServiceSubcommand::Status,
        } => catalog::status_guest().await,
        PackagesSubcommand::Checkout { source } => checkout::handle_guest(source).await,
        PackagesSubcommand::CheckoutShow { checkout_id } => catalog::show_guest(&checkout_id).await,
        PackagesSubcommand::Release => release::handle_guest().await,
        PackagesSubcommand::Cancel => checkout::cancel_guest().await,
        _ => Err(crate::error::VmError::validation(
            "This package command is restricted to the controller host",
            Some("Run package administration commands on the host"),
        )),
    }
}
