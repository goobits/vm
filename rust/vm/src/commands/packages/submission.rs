use std::collections::BTreeMap;

use vm_core::vm_progress;
use vm_packages::{
    CheckOutcome, PackageEcosystem, PackageInfrastructureClient, RegistryEndpoints, SourceKind,
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

pub(super) async fn resume_guest(
    subject: &GuestRuntime,
    checkout_id: &str,
) -> VmResult<vm_packages::SubmissionRecord> {
    let client = subject.client()?;
    let checkout = client.checkout(checkout_id).await?;
    if checkout.state != WorkflowState::Submitted
        || !checkout
            .consumers
            .iter()
            .any(|candidate| candidate == subject.consumer())
    {
        return Err(VmError::validation(
            "Checkout has no submitted generation for this consumer",
            Some("Inspect it with `vm packages show <checkout-id>`"),
        ));
    }
    let root = checkout_root(subject, checkout_id)?;
    let source = format!("{root}/source");
    ensure_clean(subject, &source)?;
    vm_progress!("Rerunning package and consumer checks for submitted changes...");
    let consumers = run_checks(subject, &client, &checkout, &source, subject.consumer()).await?;
    let submission = client.checkout_submission(checkout_id).await?;
    validate(&client, submission, consumers, "package-agent").await
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
    let root = checkout_root(subject, checkout_id)?;
    let source = format!("{root}/source");
    ensure_clean(subject, &source)?;

    vm_progress!("Running package and consumer checks...");
    let consumers = run_checks(subject, client, &checkout, &source, &consumer).await?;

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
    validate(client, submission, consumers, actor).await
}

async fn run_checks(
    subject: &impl PackageExecutor,
    client: &PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
    source: &str,
    consumer: &str,
) -> VmResult<BTreeMap<String, CheckOutcome>> {
    match checkout.source_kind {
        SourceKind::Package => {
            let definition = client.package_definition(&checkout.package).await?;
            run_package_check(subject, definition.ecosystem, source)?;
            run_consumer_check(subject, definition.ecosystem, &checkout.package, source)?;
            Ok(BTreeMap::from([(
                consumer.to_string(),
                CheckOutcome::Passed,
            )]))
        }
        SourceKind::ToolCollection => {
            run_collection_check(subject, source)?;
            Ok(BTreeMap::new())
        }
    }
}

async fn validate(
    client: &PackageInfrastructureClient,
    submission: vm_packages::SubmissionRecord,
    consumers: BTreeMap<String, CheckOutcome>,
    actor: &str,
) -> VmResult<vm_packages::SubmissionRecord> {
    let validating = client
        .validate_submission(
            &submission.submission_id,
            &ValidationRequest {
                package: CheckOutcome::Passed,
                consumers,
                actor: actor.into(),
                idempotency_key: format!(
                    "validate-{}-{}",
                    submission.submission_id,
                    generation_id(&submission.submitted_commit)
                ),
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

fn generation_id(commit: &str) -> String {
    commit.chars().take(16).collect()
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
            format!("Managed checkout has uncommitted changes: {error}"),
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

pub(super) fn run_collection_check(subject: &impl PackageExecutor, source: &str) -> VmResult<()> {
    exec(subject, ["npm", "--prefix", source, "test", "--if-present"])
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
