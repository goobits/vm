use anyhow::{bail, Context, Result};
use semver::Version;
use vm_packages::{
    BeginReleaseRequest, CleanupRequest, CompleteReleaseRequest, PackageInfrastructureClient,
    ReleaseRecord, ReleaseReworkRequest, SubmissionRecord, VersionRecommendation, WorkflowState,
};

use crate::runtime::operation_key;

pub(super) async fn begin_or_resume_release(
    client: &PackageInfrastructureClient,
    submission: &SubmissionRecord,
    request: BeginReleaseRequest,
) -> Result<ReleaseRecord> {
    match submission.state {
        WorkflowState::ReadyToRelease => Ok(client
            .begin_release(&submission.submission_id, &request)
            .await?),
        WorkflowState::Publishing => {
            let release_id = submission
                .release_id
                .as_deref()
                .context("publishing submission has no release record")?;
            let release = client.release(release_id).await?;
            verify_release(&release, &request)?;
            Ok(release)
        }
        _ => bail!("submission is not ready to release"),
    }
}

fn verify_release(release: &ReleaseRecord, expected: &BeginReleaseRequest) -> Result<()> {
    if release.version != expected.version
        || release.tag != expected.tag
        || release.source_commit != expected.source_commit
        || release.artifact_digest != expected.artifact_digest
        || release.source_pushed != expected.source_pushed
        || release.source_archive_digest != expected.source_archive_digest
        || release.registry != expected.registry
        || release.expected_publications != expected.expected_publications
    {
        bail!("retry no longer matches the durable release record");
    }
    Ok(())
}

pub(super) async fn complete_release(
    client: &PackageInfrastructureClient,
    release_id: &str,
    actor: &str,
    idempotency_key: String,
) -> Result<ReleaseRecord> {
    client
        .complete_release(
            release_id,
            &CompleteReleaseRequest {
                actor: actor.into(),
                idempotency_key,
            },
        )
        .await
}

pub(super) async fn cleanup_release(
    client: &PackageInfrastructureClient,
    release_id: &str,
    actor: &str,
) -> Result<()> {
    client
        .cleanup_release(
            release_id,
            &CleanupRequest {
                actor: actor.into(),
                idempotency_key: operation_key("cleanup", release_id),
            },
        )
        .await?;
    Ok(())
}

pub(super) fn validate_version_bump(
    previous: &Version,
    next: &Version,
    recommendation: VersionRecommendation,
) -> Result<()> {
    if next <= previous {
        bail!("release version {next} must be newer than {previous}");
    }
    let actual = if next.major > previous.major {
        VersionRecommendation::Major
    } else if next.minor > previous.minor {
        VersionRecommendation::Minor
    } else {
        VersionRecommendation::Patch
    };
    if bump_rank(actual) < bump_rank(recommendation) {
        bail!("release bump {actual:?} is smaller than the reviewed {recommendation:?} change");
    }
    Ok(())
}

pub(super) async fn validate_release_version(
    client: &PackageInfrastructureClient,
    submission: &SubmissionRecord,
    previous: &Version,
    next: &Version,
    recommendation: VersionRecommendation,
    actor: &str,
) -> Result<()> {
    let error = match validate_version_bump(previous, next, recommendation) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    let reason = error.to_string();
    client
        .request_release_rework(
            &submission.submission_id,
            &ReleaseReworkRequest {
                actor: actor.into(),
                reason: reason.clone(),
                required_followups: vec![
                    "Update the declared version, commit it, and rerun the same release command"
                        .into(),
                ],
                idempotency_key: operation_key(
                    "release-rework",
                    &format!(
                        "{}:{}",
                        submission.submission_id, submission.submitted_commit
                    ),
                ),
            },
        )
        .await
        .context("failed to return the release to its assigned package agent")?;
    Err(error)
}

const fn bump_rank(bump: VersionRecommendation) -> u8 {
    match bump {
        VersionRecommendation::Patch => 1,
        VersionRecommendation::Minor => 2,
        VersionRecommendation::Major => 3,
    }
}
