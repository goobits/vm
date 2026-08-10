use vm_core::{vm_hint, vm_progress, vm_success};
use vm_packages::WorkflowState;

use crate::error::{VmError, VmResult};

use super::{configured_state_and_client, files::ApplianceFiles, launch_job, PackageJob};

pub(super) async fn handle(
    files: &ApplianceFiles,
    submission_id: String,
    push_source: bool,
) -> VmResult<()> {
    let (state, client) = configured_state_and_client(files)?;
    let submission = client.submission(&submission_id).await?;
    if submission.state == WorkflowState::Published {
        vm_success!("Submission {submission_id} is already published");
        return Ok(());
    }
    if !matches!(
        submission.state,
        WorkflowState::ReadyToRelease | WorkflowState::Publishing
    ) {
        return Err(VmError::validation(
            "Submission is not ready to publish",
            Some("Approve, integrate, and validate it before publishing"),
        ));
    }
    if !push_source {
        vm_hint!("Release remains ready; rerun with --push-source to authorize push and publish");
        return Ok(());
    }
    vm_progress!("Launching trusted ephemeral package releaser...");
    launch_job(files, &state, PackageJob::Release(&submission_id))?;
    let published = client.submission(&submission_id).await?;
    if published.state != WorkflowState::Published {
        return Err(VmError::validation(
            "Release job ended before publication completed",
            Some("Retry the same publish command; release operations are idempotent"),
        ));
    }
    let release_id = published.release_id.as_deref().ok_or_else(|| {
        VmError::validation("Published release record is missing", None::<String>)
    })?;
    let release = client.release(release_id).await?;
    vm_success!(
        "Published {}@{} from {}",
        release.package,
        release.version,
        release.source_commit
    );
    Ok(())
}
