use std::path::PathBuf;

use vm_core::{vm_hint, vm_progress, vm_success};
use vm_packages::WorkflowState;

use crate::error::{VmError, VmResult};

use super::{checkout, configured_state_and_client, files::ApplianceFiles, launch_job, PackageJob};

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
    ) {
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
