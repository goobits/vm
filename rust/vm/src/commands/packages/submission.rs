use std::collections::BTreeMap;

use vm_core::vm_progress;
use vm_packages::{
    CheckOutcome, PackageEcosystem, PackageInfrastructureClient, RegistryEndpoints,
    ValidationRequest, WorkflowState,
};

use crate::error::{VmError, VmResult};

use super::{
    overrides::cargo_patch,
    runtime::{checkout_root, exec, exec_in_workspace, GuestRuntime, PackageExecutor},
};

pub(super) async fn handle_guest(
    subject: &GuestRuntime,
    checkout_id: &str,
) -> VmResult<vm_packages::SubmissionRecord> {
    submit(
        subject,
        &subject.client()?,
        subject.gateway(),
        checkout_id,
        subject.consumer().to_string(),
        "package-agent",
    )
    .await
}

async fn submit(
    subject: &impl PackageExecutor,
    client: &PackageInfrastructureClient,
    gateway: &str,
    checkout_id: &str,
    consumer: String,
    actor: &str,
) -> VmResult<vm_packages::SubmissionRecord> {
    let checkout = client.checkout(checkout_id).await?;
    if !matches!(
        checkout.state,
        WorkflowState::Active | WorkflowState::NeedsChanges
    ) || !checkout
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
    let root = checkout_root(subject, checkout_id)?;
    let source = format!("{root}/source");
    ensure_clean(subject, &source)?;

    vm_progress!("Running package and consumer checks...");
    run_package_check(subject, definition.ecosystem, &source)?;
    run_consumer_check(subject, definition.ecosystem, &checkout.package, &source)?;

    let bundle = format!("{root}/submission.bundle");
    exec(subject, ["rm", "-f", bundle.as_str()])?;
    exec(
        subject,
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
    let upload_client =
        PackageInfrastructureClient::new(RegistryEndpoints::new(gateway).map_err(VmError::from)?);
    let upload_url = upload_client.submission_upload_url(checkout_id, &consumer);
    exec(
        subject,
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
    exec(subject, ["rm", "-f", bundle.as_str()])?;

    let submission = client.checkout_submission(checkout_id).await?;
    let validating = client
        .validate_submission(
            &submission.submission_id,
            &ValidationRequest {
                package: CheckOutcome::Passed,
                consumers: BTreeMap::from([(consumer, CheckOutcome::Passed)]),
                actor: actor.into(),
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
    Ok(validating)
}

fn ensure_clean(subject: &impl PackageExecutor, source: &str) -> VmResult<()> {
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
    subject: &impl PackageExecutor,
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
    subject: &impl PackageExecutor,
    ecosystem: PackageEcosystem,
    package: &str,
    source: &str,
) -> VmResult<()> {
    match ecosystem {
        PackageEcosystem::Npm => exec_in_workspace(subject, ["npm", "test", "--if-present"]),
        PackageEcosystem::Cargo => {
            let patch = cargo_patch(package, source);
            exec_in_workspace(subject, ["cargo", "test", "--config", &patch])
        }
        PackageEcosystem::Python => exec_in_workspace(subject, ["python", "-m", "pytest"]),
    }
}
