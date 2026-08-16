use std::collections::BTreeMap;

use vm_core::{vm_progress, vm_success};
use vm_packages::{
    CheckOutcome, IntegrationRequest, PackageInfrastructureClient, RegistryEndpoints, SourceKind,
    ValidationRequest, WorkflowState,
};

use crate::error::{VmError, VmResult};

use super::{
    runtime::{checkout_root, exec, GuestRuntime},
    submission::{run_binary_check, run_collection_check, run_consumer_check, run_package_check},
};

pub(super) async fn handle_guest(
    subject: &GuestRuntime,
    submission_id: &str,
) -> VmResult<vm_packages::SubmissionRecord> {
    integrate(
        subject,
        &subject.client()?,
        subject.gateway(),
        submission_id,
        subject.consumer().to_string(),
        "rebase".into(),
        "package-agent",
    )
    .await
}

async fn integrate(
    subject: &GuestRuntime,
    client: &PackageInfrastructureClient,
    gateway: &str,
    submission_id: &str,
    consumer: String,
    strategy: String,
    actor: &str,
) -> VmResult<vm_packages::SubmissionRecord> {
    let submission = client.submission(submission_id).await?;
    if !matches!(
        submission.state,
        WorkflowState::Approved | WorkflowState::Integrating
    ) {
        return Err(VmError::validation(
            "Submission is not approved for integration",
            Some("Submit it and resolve any integration-review feedback first"),
        ));
    }
    let checkout = client.checkout(&submission.checkout_id).await?;
    if !checkout
        .consumers
        .iter()
        .any(|candidate| candidate == &consumer)
    {
        return Err(VmError::validation(
            "Submission is not assigned to this consumer",
            None::<String>,
        ));
    }
    vm_progress!("Integrating against the latest canonical package revision...");
    let integrating = client
        .prepare_integration(
            &submission.submission_id,
            &IntegrationRequest {
                actor: actor.into(),
                strategy,
                idempotency_key: format!(
                    "integrate-{}-{}",
                    submission.submission_id,
                    generation_id(&submission.submitted_commit)
                ),
            },
        )
        .await?;
    let integration = integrating
        .integration
        .as_ref()
        .ok_or_else(|| VmError::validation("Integration record is missing", None::<String>))?;
    let checkout_root = checkout_root(subject, &checkout.checkout_id)?;
    let root = format!("{checkout_root}/integration-{}", integrating.submission_id);
    let source = format!("{root}/source");
    let bundle = format!("{root}/integration.bundle");
    exec(subject, ["mkdir", "-p", root.as_str()])?;
    let download_client =
        PackageInfrastructureClient::new(RegistryEndpoints::new(gateway).map_err(VmError::from)?);
    let url = download_client.integration_bundle_url(&integrating.submission_id, &consumer);
    exec(
        subject,
        [
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--header",
            &format!("@{checkout_root}/authorization-header"),
            &url,
            "--output",
            &bundle,
        ],
    )?;
    exec(subject, ["git", "clone", &bundle, source.as_str()])?;
    exec(
        subject,
        [
            "git",
            "-C",
            source.as_str(),
            "checkout",
            "--detach",
            &integration.integration_commit,
        ],
    )?;
    exec(subject, ["rm", "-f", bundle.as_str()])?;
    vm_progress!("Rerunning integrated package and consumer checks...");
    let consumers = match checkout.source_kind {
        SourceKind::Package => {
            let definition = client.package_definition(&submission.package).await?;
            run_package_check(subject, definition.ecosystem, &source)?;
            if checkout.workspace_release || checkout.source_only {
                BTreeMap::new()
            } else {
                run_consumer_check(subject, definition.ecosystem, &submission.package, &source)?;
                BTreeMap::from([(consumer, CheckOutcome::Passed)])
            }
        }
        SourceKind::ToolBinary => {
            run_binary_check(subject, &source)?;
            BTreeMap::new()
        }
        SourceKind::ToolCollection => {
            run_collection_check(subject, &source)?;
            BTreeMap::new()
        }
    };
    let integration_commit = integration.integration_commit.clone();
    let ready = client
        .complete_integration(
            &integrating.submission_id,
            &ValidationRequest {
                package: CheckOutcome::Passed,
                consumers,
                actor: actor.into(),
                idempotency_key: format!(
                    "integration-checks-{}-{}",
                    integrating.submission_id,
                    generation_id(&integrating.submitted_commit)
                ),
            },
        )
        .await?;
    exec(subject, ["rm", "-rf", "--", root.as_str()])?;
    vm_success!("Integrated checks passed at {integration_commit}");
    Ok(ready)
}

fn generation_id(commit: &str) -> String {
    commit.chars().take(16).collect()
}
