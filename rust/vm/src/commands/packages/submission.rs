use std::collections::BTreeMap;

use vm_core::vm_progress;
use vm_packages::{
    CheckOutcome, PackageEcosystem, PackageInfrastructureClient, SourceKind, ValidationRequest,
    WorkflowState,
};

use crate::error::{VmError, VmResult};

use super::{
    guest_checkout::{checkout_root, remove_directory, remove_file},
    guest_runtime::{exec, exec_in_workspace, exec_output, GuestRuntime},
    overrides::cargo_patch,
};

pub(super) async fn handle_guest(
    subject: &GuestRuntime,
    client: &PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
    package_ecosystem: Option<PackageEcosystem>,
) -> VmResult<vm_packages::SubmissionRecord> {
    submit(
        subject,
        client,
        checkout,
        subject.consumer().to_string(),
        "package-agent",
        package_ecosystem,
    )
    .await
}

pub(super) async fn handle_workspace(
    subject: &GuestRuntime,
    client: &PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
    source: &str,
    package_ecosystem: Option<PackageEcosystem>,
) -> VmResult<vm_packages::SubmissionRecord> {
    if !checkout.workspace_release
        || !matches!(
            checkout.state,
            WorkflowState::Created | WorkflowState::Active | WorkflowState::NeedsChanges
        )
    {
        return Err(VmError::validation(
            "Canonical workspace release is not ready for submission",
            Some("Rerun `vm packages release` after repairing package infrastructure"),
        ));
    }
    ensure_tracked_clean(subject, source, "Canonical workspace")?;
    let commit = exec_output(subject, ["git", "-C", source, "rev-parse", "HEAD"])?;
    submit_workspace_commit(
        subject,
        client,
        checkout,
        source,
        commit.trim(),
        None,
        package_ecosystem,
    )
    .await
}

pub(super) async fn resume_workspace(
    subject: &GuestRuntime,
    client: &PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
    source: &str,
    package_ecosystem: Option<PackageEcosystem>,
) -> VmResult<vm_packages::SubmissionRecord> {
    let submission = client.checkout_submission(&checkout.checkout_id).await?;
    let submitted_commit = submission.submitted_commit.clone();
    submit_workspace_commit(
        subject,
        client,
        checkout,
        source,
        &submitted_commit,
        Some(submission),
        package_ecosystem,
    )
    .await
}

pub(super) async fn resume_guest(
    subject: &GuestRuntime,
    client: &PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
    package_ecosystem: Option<PackageEcosystem>,
) -> VmResult<vm_packages::SubmissionRecord> {
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
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let source = format!("{root}/source");
    ensure_clean(subject, &source, "Managed checkout")?;
    vm_progress!("Rerunning package and consumer checks for submitted changes...");
    let consumers = run_checks(
        subject,
        checkout,
        &source,
        subject.consumer(),
        package_ecosystem,
    )?;
    let submission = client.checkout_submission(&checkout.checkout_id).await?;
    validate(client, submission, consumers, "package-agent").await
}

async fn submit(
    subject: &GuestRuntime,
    client: &PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
    consumer: String,
    actor: &str,
    package_ecosystem: Option<PackageEcosystem>,
) -> VmResult<vm_packages::SubmissionRecord> {
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
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let source = format!("{root}/source");
    ensure_clean(subject, &source, "Managed checkout")?;

    vm_progress!("Running package and consumer checks...");
    let consumers = run_checks(subject, checkout, &source, &consumer, package_ecosystem)?;

    let bundle = format!("{root}/submission.bundle");
    remove_file(&bundle)?;
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
    let submission = upload_bundle(
        subject,
        client,
        &checkout.checkout_id,
        &consumer,
        &root,
        &bundle,
    )?;
    remove_file(&bundle)?;

    validate(client, submission, consumers, actor).await
}

fn run_checks(
    subject: &GuestRuntime,
    checkout: &vm_packages::CheckoutRecord,
    source: &str,
    consumer: &str,
    package_ecosystem: Option<PackageEcosystem>,
) -> VmResult<BTreeMap<String, CheckOutcome>> {
    match checkout.source_kind {
        SourceKind::Package => {
            let ecosystem = package_ecosystem.ok_or_else(|| {
                VmError::validation(
                    "Package release context has no ecosystem",
                    Some("Rerun `vm packages release` after repairing package infrastructure"),
                )
            })?;
            run_package_check(subject, ecosystem, source)?;
            if checkout.workspace_release || checkout.source_only {
                Ok(BTreeMap::new())
            } else {
                run_consumer_check(subject, ecosystem, &checkout.package, source)?;
                Ok(BTreeMap::from([(
                    consumer.to_string(),
                    CheckOutcome::Passed,
                )]))
            }
        }
        SourceKind::ToolBinary => {
            run_binary_check(subject, source)?;
            Ok(BTreeMap::new())
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

fn ensure_clean(subject: &GuestRuntime, source: &str, label: &str) -> VmResult<()> {
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
            format!("{label} has uncommitted changes: {error}"),
            Some("Commit intended files and remove unintended files before submitting"),
        )
    })
}

fn ensure_tracked_clean(subject: &GuestRuntime, source: &str, label: &str) -> VmResult<()> {
    exec(
        subject,
        [
            "/bin/sh",
            "-c",
            "git -C \"$1\" diff --quiet -- && git -C \"$1\" diff --cached --quiet --",
            "vm-package-clean",
            source,
        ],
    )
    .map_err(|error| {
        VmError::validation(
            format!("{label} has uncommitted tracked changes: {error}"),
            Some("Commit intended tracked files before submitting"),
        )
    })
}

async fn submit_workspace_commit(
    subject: &GuestRuntime,
    client: &PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
    source: &str,
    commit: &str,
    existing_submission: Option<vm_packages::SubmissionRecord>,
    package_ecosystem: Option<PackageEcosystem>,
) -> VmResult<vm_packages::SubmissionRecord> {
    let current = exec_output(subject, ["git", "-C", source, "rev-parse", "HEAD"])?;
    if current.trim() != commit {
        return Err(VmError::validation(
            "Canonical workspace HEAD changed while its release is in progress",
            Some("Finish the active commit with `vm packages release` before releasing another commit"),
        ));
    }
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let bundle = format!("{root}/submission.bundle");
    let scratch = format!("{root}/workspace-validation");
    remove_directory(&scratch)?;
    remove_file(&bundle)?;
    let result = async {
        exec(
            subject,
            [
                "git",
                "-C",
                source,
                "bundle",
                "create",
                bundle.as_str(),
                "HEAD",
            ],
        )?;
        exec(subject, ["git", "clone", bundle.as_str(), scratch.as_str()])?;
        vm_progress!("Running checks from an isolated copy of the canonical workspace...");
        let consumers = run_checks(
            subject,
            checkout,
            &scratch,
            subject.consumer(),
            package_ecosystem,
        )?;
        let submission = if let Some(submission) = existing_submission {
            submission
        } else {
            upload_bundle(
                subject,
                client,
                &checkout.checkout_id,
                subject.consumer(),
                &root,
                &bundle,
            )?
        };
        Ok::<_, VmError>((submission, consumers))
    }
    .await;
    let scratch_cleanup = remove_directory(&scratch);
    let bundle_cleanup = remove_file(&bundle);
    let (submission, consumers) = result?;
    scratch_cleanup?;
    bundle_cleanup?;
    validate(client, submission, consumers, "workspace-agent").await
}

fn upload_bundle(
    subject: &GuestRuntime,
    client: &PackageInfrastructureClient,
    checkout_id: &str,
    consumer: &str,
    root: &str,
    bundle: &str,
) -> VmResult<vm_packages::SubmissionRecord> {
    let upload_url = client.submission_upload_url(checkout_id, consumer);
    let response = exec_output(
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
            &format!("@{root}/agent-capability-header"),
            "--header",
            "Content-Type: application/x-git-bundle",
            "--data-binary",
            &format!("@{bundle}"),
            &upload_url,
        ],
    )?;
    decode_submission(&response)
}

fn decode_submission(response: &str) -> VmResult<vm_packages::SubmissionRecord> {
    serde_json::from_str(response).map_err(|error| {
        VmError::general(
            error,
            "Package infrastructure returned an invalid submission response",
        )
    })
}

pub(super) fn run_package_check(
    subject: &GuestRuntime,
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

pub(super) fn run_collection_check(subject: &GuestRuntime, source: &str) -> VmResult<()> {
    exec(subject, ["npm", "--prefix", source, "test", "--if-present"])
}

pub(super) fn run_binary_check(subject: &GuestRuntime, source: &str) -> VmResult<()> {
    let content = exec_output(subject, ["git", "-C", source, "show", "HEAD:vm-tool.yaml"])?;
    let manifest: vm_packages::ToolSourceManifest =
        serde_yaml_ng::from_str(&content).map_err(|error| {
            VmError::validation(format!("Invalid vm-tool.yaml: {error}"), None::<String>)
        })?;
    if manifest.kind != vm_packages::ToolKind::Binary {
        return Err(VmError::validation(
            "Binary tool checkout has a non-binary vm-tool.yaml",
            None::<String>,
        ));
    }
    manifest.validate().map_err(VmError::from)
}

pub(super) fn run_consumer_check(
    subject: &GuestRuntime,
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

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::decode_submission;
    use vm_packages::{SubmissionRecord, WorkflowState};

    #[test]
    fn submission_upload_response_is_reused_directly() {
        let expected = SubmissionRecord {
            submission_id: "submission-1".into(),
            checkout_id: "checkout-1".into(),
            package: "shared".into(),
            branch: "agents/project-a/checkout-1".into(),
            base_commit: "a".repeat(40),
            submitted_commit: "b".repeat(40),
            diff_digest: "c".repeat(64),
            state: WorkflowState::Submitted,
            validation: None,
            review: None,
            integration: None,
            release_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let response = serde_json::to_string(&expected).unwrap();

        assert_eq!(decode_submission(&response).unwrap(), expected);
        assert!(decode_submission("not-json").is_err());
    }
}
