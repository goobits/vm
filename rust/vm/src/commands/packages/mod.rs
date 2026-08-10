mod appliance;
mod catalog;
mod checkout;
mod consumer;
mod docker;
mod files;
mod integration;
mod process;
mod release;
mod runtime;
mod submission;
mod tart;

use std::path::PathBuf;

use crate::cli::PackagesSubcommand;
use crate::error::VmResult;
use vm_core::{vm_hint, vm_success};

use appliance::configured_client;
use files::ApplianceFiles;
pub(super) use runtime::apply_client_environment;

pub(super) async fn handle(
    command: PackagesSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let files = ApplianceFiles::discover()?;
    let _operation_lock = match &command {
        PackagesSubcommand::Backups { .. }
        | PackagesSubcommand::Backup { .. }
        | PackagesSubcommand::Restore { .. } => None,
        PackagesSubcommand::Up { .. } | PackagesSubcommand::Down { .. } => {
            Some(files.acquire_lifecycle_lock()?)
        }
        _ => Some(files.acquire_operation_lock()?),
    };
    match command {
        PackagesSubcommand::Up {
            runtime,
            port,
            registry_image,
            job_image,
        } => appliance::up(&files, runtime, port, registry_image, job_image).await,
        PackagesSubcommand::Down { runtime } => appliance::down(&files, runtime),
        PackagesSubcommand::Status { runtime } => appliance::status(&files, runtime).await,
        PackagesSubcommand::Doctor { runtime } => appliance::doctor(&files, runtime).await,
        PackagesSubcommand::Backups { runtime } => appliance::list_backups(&files, runtime),
        PackagesSubcommand::Backup { runtime } => appliance::backup(&files, runtime),
        PackagesSubcommand::Restore { backup_id, runtime } => {
            appliance::restore(&files, runtime, &backup_id)
        }
        PackagesSubcommand::Register {
            name,
            ecosystem,
            repository,
            branch,
            ci_registry,
        } => catalog::register(&files, name, ecosystem, repository, branch, ci_registry).await,
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
        PackagesSubcommand::Cancel { checkout_id } => {
            cancel_checkout(&files, config_path, profile, &checkout_id).await
        }
        PackagesSubcommand::Cleanup { checkout_id } => {
            cleanup_checkout(&files, config_path, profile, &checkout_id).await
        }
        PackagesSubcommand::Submit {
            checkout_id,
            consumer,
        } => submission::handle(&files, config_path, profile, checkout_id, consumer).await,
        PackagesSubcommand::Integrate {
            submission_id,
            consumer,
            strategy,
        } => {
            integration::handle(
                &files,
                config_path,
                profile,
                submission_id,
                consumer,
                strategy,
            )
            .await
        }
        PackagesSubcommand::Publish {
            submission_id,
            push_source,
        } => release::handle(&files, config_path, profile, submission_id, push_source).await,
        PackagesSubcommand::Rollout { target, consumer } => {
            consumer::rollout(&files, target, consumer).await
        }
        PackagesSubcommand::Auth {
            token_file,
            ci_token_file,
            clear,
            clear_ci,
        } => catalog::configure_auth(&files, token_file, ci_token_file, clear, clear_ci),
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
    let closed = client
        .cleanup_checkout(
            checkout_id,
            &vm_packages::CleanupRequest {
                actor: "vm-controller".into(),
                idempotency_key: format!("cleanup-{checkout_id}"),
            },
        )
        .await?;
    if let Err(error) = checkout::cleanup_local(config_path, profile, &checkout) {
        vm_hint!("Service checkout closed; local temporary data was unavailable: {error}");
    }
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
