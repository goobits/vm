use std::collections::BTreeMap;
use std::path::PathBuf;

use vm_core::{vm_progress, vm_success};
use vm_packages::{
    CheckOutcome, PackageEcosystem, PackageInfrastructureClient, RegistryEndpoints,
    ValidationRequest, WorkflowState,
};

use crate::commands::command_context::{load_runtime_subject, project_name, RuntimeSubject};
use crate::error::{VmError, VmResult};

use super::{
    configured_state_and_client,
    files::ApplianceFiles,
    gateway_for_provider, launch_review,
    runtime::{checkout_root, exec, exec_in_workspace},
};

pub(super) async fn handle(
    files: &ApplianceFiles,
    config_path: Option<PathBuf>,
    profile: Option<String>,
    checkout_id: String,
    requested_consumer: Option<String>,
) -> VmResult<()> {
    let subject = load_runtime_subject(config_path, profile, None)?;
    let current_project = project_name(&subject.config).to_string();
    let consumer = requested_consumer.unwrap_or_else(|| current_project.clone());
    if consumer != current_project {
        return Err(VmError::validation(
            format!("Consumer '{consumer}' is not the current project '{current_project}'"),
            Some("Run this command from the selected consumer project"),
        ));
    }

    let (state, client) = configured_state_and_client(files)?;
    let checkout = client.checkout(&checkout_id).await?;
    if checkout.state != WorkflowState::Active
        || !checkout
            .consumers
            .iter()
            .any(|candidate| candidate == &consumer)
    {
        return Err(VmError::validation(
            "Checkout is not active for this consumer",
            Some("Inspect it with `vm packages show <checkout-id>`"),
        ));
    }
    let definition = client.package_definition(&checkout.package).await?;
    let root = checkout_root(&checkout_id);
    let source = format!("{root}/source");
    ensure_clean(&subject, &source)?;

    vm_progress!("Running package and consumer checks...");
    run_package_check(&subject, definition.ecosystem, &source)?;
    run_consumer_check(&subject, definition.ecosystem, &checkout.package, &source)?;

    let bundle = format!("{root}/submission.bundle");
    exec(&subject, ["rm", "-f", bundle.as_str()])?;
    exec(
        &subject,
        [
            "git",
            "-C",
            source.as_str(),
            "bundle",
            "create",
            bundle.as_str(),
            "--all",
        ],
    )?;
    let gateway = gateway_for_provider(&state, subject.provider.name())?;
    let upload_client =
        PackageInfrastructureClient::new(RegistryEndpoints::new(gateway).map_err(VmError::from)?);
    let upload_url = upload_client.submission_upload_url(&checkout_id, &consumer);
    exec(
        &subject,
        [
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--request",
            "POST",
            "--header",
            &format!("@{root}/authorization-header"),
            "--header",
            "Content-Type: application/x-git-bundle",
            "--data-binary",
            &format!("@{bundle}"),
            &upload_url,
        ],
    )?;
    exec(&subject, ["rm", "-f", bundle.as_str()])?;

    let submission = client.checkout_submission(&checkout_id).await?;
    let validating = client
        .validate_submission(
            &submission.submission_id,
            &ValidationRequest {
                package: CheckOutcome::Passed,
                consumers: BTreeMap::from([(consumer, CheckOutcome::Passed)]),
                actor: "vm-controller".into(),
                idempotency_key: format!("validate-{}", submission.submission_id),
            },
        )
        .await?;
    if validating.state != WorkflowState::Reviewing {
        return Err(VmError::validation(
            "Submission validation failed",
            Some("Inspect the submission before retrying"),
        ));
    }
    vm_progress!("Launching ephemeral integration reviewer...");
    launch_review(files, &state, &validating.submission_id)?;
    let reviewed = client.submission(&validating.submission_id).await?;
    vm_success!(
        "Submission {} review: {}",
        reviewed.submission_id,
        reviewed
            .review
            .as_ref()
            .map_or("unavailable".into(), |review| format!(
                "{:?}",
                review.decision
            )
            .to_ascii_lowercase())
    );
    Ok(())
}

fn ensure_clean(subject: &RuntimeSubject, source: &str) -> VmResult<()> {
    exec(
        subject,
        [
            "/bin/sh",
            "-c",
            "test -z \"$(git -C \"$1\" status --porcelain)\"",
            "vm-package-clean",
            source,
        ],
    )
    .map_err(|error| {
        VmError::validation(
            format!("Package checkout has uncommitted changes: {error}"),
            Some("Commit intended files and remove unintended files before submitting"),
        )
    })
}

pub(super) fn run_package_check(
    subject: &RuntimeSubject,
    ecosystem: PackageEcosystem,
    source: &str,
) -> VmResult<()> {
    match ecosystem {
        PackageEcosystem::Npm => exec(subject, ["npm", "--prefix", source, "test", "--if-present"]),
        PackageEcosystem::Cargo => exec(
            subject,
            [
                "cargo",
                "test",
                "--manifest-path",
                &format!("{source}/Cargo.toml"),
            ],
        ),
        PackageEcosystem::Python => exec(subject, ["python", "-m", "pytest", source]),
    }
}

pub(super) fn run_consumer_check(
    subject: &RuntimeSubject,
    ecosystem: PackageEcosystem,
    package: &str,
    source: &str,
) -> VmResult<()> {
    match ecosystem {
        PackageEcosystem::Npm => exec_in_workspace(subject, ["npm", "test", "--if-present"]),
        PackageEcosystem::Cargo => {
            let patch = format!("patch.crates-io.{package}.path=\"{source}\"");
            exec_in_workspace(subject, ["cargo", "test", "--config", &patch])
        }
        PackageEcosystem::Python => exec_in_workspace(subject, ["python", "-m", "pytest"]),
    }
}
