use std::path::PathBuf;

use vm_core::{vm_hint, vm_progress, vm_success};
use vm_packages::{LeaseRequest, WorkflowState};

use crate::error::{VmError, VmResult};

use super::{
    appliance::{configured_state_and_client, launch_job, PackageJob},
    checkout,
    files::ApplianceFiles,
    integration,
    runtime::{checkout_root, exec_output, GuestRuntime},
    submission,
};

const RELEASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) async fn handle(
    files: &ApplianceFiles,
    config_path: Option<PathBuf>,
    profile: Option<String>,
    submission_id: String,
    push_source: bool,
) -> VmResult<()> {
    let (state, client) = configured_state_and_client(files)?;
    let submission = client.submission(&submission_id).await?;
    let already_closed = submission.state == WorkflowState::Closed;
    if !matches!(
        submission.state,
        WorkflowState::ReadyToRelease
            | WorkflowState::Publishing
            | WorkflowState::Published
            | WorkflowState::Closed
    ) {
        return Err(VmError::validation(
            "Submission is not ready to publish",
            Some("Approve, integrate, and validate it before publishing"),
        ));
    }
    if !push_source
        && !matches!(
            submission.state,
            WorkflowState::Published | WorkflowState::Closed
        )
    {
        vm_hint!("Release remains ready; rerun with --push-source to authorize push and publish");
        return Ok(());
    }
    if !already_closed {
        vm_progress!("Launching trusted ephemeral package releaser...");
        launch_job(files, &state, PackageJob::Release(&submission_id))?;
    }
    let published = client.submission(&submission_id).await?;
    if !matches!(
        published.state,
        WorkflowState::Published | WorkflowState::Closed
    ) {
        return Err(VmError::validation(
            "Release job ended before publication completed",
            Some("Retry the same publish command; release operations are idempotent"),
        ));
    }
    let release_id = published.release_id.as_deref().ok_or_else(|| {
        VmError::validation("Published release record is missing", None::<String>)
    })?;
    let release = client.release(release_id).await?;
    if let Err(error) = checkout::cleanup_local(
        config_path,
        profile,
        &client.checkout(&published.checkout_id).await?,
    )
    .await
    {
        vm_hint!("Published successfully; local temporary checkout cleanup was skipped: {error}");
    }
    vm_success!(
        "Published {}@{} from {}",
        release.package,
        release.version,
        release.source_commit
    );
    Ok(())
}

pub(super) async fn handle_guest(checkout_id: &str) -> VmResult<()> {
    let subject = GuestRuntime::discover()?;
    let client = subject.client()?;
    let checkout = client.checkout(checkout_id).await?;
    if !checkout.consumers.contains(&subject.consumer().to_string()) {
        return Err(VmError::validation(
            "Checkout is not assigned to this managed environment",
            None::<String>,
        ));
    }
    renew_release_lease(&subject, &client, &checkout).await?;
    let mut current = match checkout.state {
        WorkflowState::Active | WorkflowState::NeedsChanges => {
            submission::handle_guest(&subject, checkout_id).await?
        }
        WorkflowState::Submitted
        | WorkflowState::Validating
        | WorkflowState::Reviewing
        | WorkflowState::Approved
        | WorkflowState::Integrating
        | WorkflowState::ReadyToRelease
        | WorkflowState::Publishing
        | WorkflowState::Published
        | WorkflowState::Closed => client.checkout_submission(checkout_id).await?,
        state => {
            return Err(VmError::validation(
                format!("Checkout cannot be released from {state:?}"),
                Some("Inspect it with `vm packages show <checkout-id>`"),
            ))
        }
    };
    current = wait_for_review(&client, current).await?;
    if matches!(
        current.state,
        WorkflowState::Approved | WorkflowState::Integrating
    ) {
        current = integration::handle_guest(&subject, &current.submission_id).await?;
    }
    let published = wait_for_publication(&client, current).await?;
    let release_id = published.release_id.as_deref().ok_or_else(|| {
        VmError::validation("Published submission has no release record", None::<String>)
    })?;
    let release = client.release(release_id).await?;
    let checkout = client.checkout(checkout_id).await?;
    if let Err(error) = checkout::cleanup_guest(&subject, &checkout) {
        vm_hint!("Published successfully; local checkout cleanup was skipped: {error}");
    }
    vm_success!(
        "Released {}@{} from {}",
        release.package,
        release.version,
        release.source_commit
    );
    Ok(())
}

async fn renew_release_lease(
    subject: &GuestRuntime,
    client: &vm_packages::PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<()> {
    if checkout.lease.is_none() {
        return Ok(());
    }
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let header = exec_output(subject, ["cat", &format!("{root}/authorization-header")])?;
    let token = header
        .trim()
        .strip_prefix("Authorization: Bearer ")
        .ok_or_else(|| {
            VmError::validation(
                "Managed checkout lease credential is invalid",
                None::<String>,
            )
        })?;
    client
        .renew_lease(
            &checkout.checkout_id,
            &LeaseRequest {
                holder: checkout.agent.clone(),
                lease_token: token.into(),
                duration_seconds: 24 * 60 * 60,
                idempotency_key: format!("release-lease-{}", checkout.checkout_id),
            },
        )
        .await?;
    Ok(())
}

async fn wait_for_review(
    client: &vm_packages::PackageInfrastructureClient,
    mut submission: vm_packages::SubmissionRecord,
) -> VmResult<vm_packages::SubmissionRecord> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    while matches!(
        submission.state,
        WorkflowState::Submitted | WorkflowState::Validating | WorkflowState::Reviewing
    ) {
        if tokio::time::Instant::now() >= deadline {
            return Err(VmError::validation(
                "Timed out waiting for package review",
                Some("Rerun the same release command to resume"),
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        submission = client.submission(&submission.submission_id).await?;
    }
    match submission.state {
        WorkflowState::NeedsChanges => Err(VmError::validation(
            submission
                .review
                .as_ref()
                .map(|review| format!("Package review requested changes: {}", review.reason))
                .unwrap_or_else(|| "Package review requested changes".into()),
            Some("Edit and commit the checkout, then rerun the same release command"),
        )),
        WorkflowState::Rejected | WorkflowState::Failed => Err(VmError::validation(
            "Package review rejected or failed the release",
            Some("Inspect the checkout and review receipt"),
        )),
        _ => Ok(submission),
    }
}

async fn wait_for_publication(
    client: &vm_packages::PackageInfrastructureClient,
    mut submission: vm_packages::SubmissionRecord,
) -> VmResult<vm_packages::SubmissionRecord> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    while matches!(
        submission.state,
        WorkflowState::ReadyToRelease | WorkflowState::Publishing
    ) {
        if tokio::time::Instant::now() >= deadline {
            return Err(VmError::validation(
                "Timed out waiting for private package publication",
                Some("Rerun the same release command to resume"),
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        submission = client.submission(&submission.submission_id).await?;
    }
    if matches!(
        submission.state,
        WorkflowState::Published | WorkflowState::Closed
    ) {
        Ok(submission)
    } else {
        Err(VmError::validation(
            format!("Package release stopped in {:?}", submission.state),
            Some("Inspect package infrastructure logs and rerun when repaired"),
        ))
    }
}
