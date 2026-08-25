//! Deterministic submission review kept separate from the queue worker.

mod checkout;
mod checks;
mod public_api;

use anyhow::{bail, Result};
use vm_packages::{
    PackageEcosystem, PackageInfrastructureClient, PublicApiDiff, ReviewDecision, ReviewRequest,
    SourceKind, VersionRecommendation, WorkflowState,
};

use crate::runtime::operation_key;
use checkout::ReviewSource;
use checks::{generated_path, run_required_checks, sensitive_path};
use public_api::{manifest_has_public_changes, public_api_paths, removed_public_surface};

pub async fn review_submission(
    client: &PackageInfrastructureClient,
    token: &str,
    submission_id: &str,
) -> Result<()> {
    let submission = client.submission(submission_id).await?;
    if submission.state != WorkflowState::Reviewing
        || !submission
            .validation
            .as_ref()
            .is_some_and(|result| result.passed())
    {
        bail!("submission is not ready for review");
    }

    let checkout = client.checkout(&submission.checkout_id).await?;
    let ecosystem = match checkout.source_kind {
        SourceKind::Package => Some(
            client
                .package_definition(&submission.package)
                .await?
                .ecosystem,
        ),
        SourceKind::ToolBinary | SourceKind::ToolCollection => None,
    };
    let source = ReviewSource::prepare(client, token, &submission, &checkout)?;
    let manifest_is_public = manifest_has_public_changes(
        checkout.source_kind,
        ecosystem,
        source.path(),
        checkout.initial_release,
        &submission.base_commit,
        &submission.submitted_commit,
        source.changed_paths(),
    )?;
    let api_paths = public_api_paths(
        checkout.source_kind,
        ecosystem,
        source.changed_paths(),
        manifest_is_public,
    );
    let potentially_breaking = removed_public_surface(source.diff());
    let api_diff = PublicApiDiff {
        changed_paths: api_paths.clone(),
        potentially_breaking,
    };

    let (decision, reason, required_followups) = review_decision(
        checkout.source_kind,
        ecosystem,
        source.path(),
        source.changed_paths(),
        &api_paths,
    )?;
    let recommended_version = if potentially_breaking {
        VersionRecommendation::Major
    } else if api_paths.is_empty() {
        VersionRecommendation::Patch
    } else {
        VersionRecommendation::Minor
    };
    let result = client
        .record_review(
            &submission.submission_id,
            &ReviewRequest {
                decision,
                recommended_version,
                api_diff,
                reason,
                required_followups,
                merge_strategy: "rebase".into(),
                reviewer: "ephemeral-integration-agent".into(),
                idempotency_key: operation_key(
                    "review",
                    &format!(
                        "{}:{}",
                        submission.submission_id, submission.submitted_commit
                    ),
                ),
            },
        )
        .await?;
    tracing::info!(
        operation = "review",
        submission_id = %result.submission_id,
        decision = ?decision,
        "package review completed"
    );
    Ok(())
}

fn review_decision(
    source_kind: SourceKind,
    ecosystem: Option<PackageEcosystem>,
    source: &std::path::Path,
    changed_paths: &[String],
    api_paths: &[String],
) -> Result<(ReviewDecision, String, Vec<String>)> {
    if let Some(path) = sensitive_path(source, changed_paths) {
        return Ok((
            ReviewDecision::Reject,
            format!("sensitive file included: {path}"),
            vec!["Remove credentials or private files from the submission".into()],
        ));
    }
    if let Some(path) = generated_path(changed_paths) {
        return Ok((
            ReviewDecision::NeedsChanges,
            format!("generated dependency/build output included: {path}"),
            vec!["Remove generated files from the submission".into()],
        ));
    }
    if !run_required_checks(source_kind, ecosystem, source)? {
        return Ok((
            ReviewDecision::NeedsChanges,
            "required package checks failed in the isolated reviewer".into(),
            vec!["Fix package checks and resubmit".into()],
        ));
    }
    Ok((
        ReviewDecision::Approve,
        if api_paths.is_empty() {
            "checks passed; no public API paths changed".into()
        } else {
            format!(
                "checks passed; {} public API path(s) changed",
                api_paths.len()
            )
        },
        Vec::new(),
    ))
}
