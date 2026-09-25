use std::collections::BTreeMap;

use super::*;
use crate::ImportedSubmission;
use vm_packages::{
    BeginReleaseRequest, CheckOutcome, CreateCheckout, IntegrationRecord, PublicApiDiff,
    RegisterTool, ReviewRequest, ToolBuildArtifact, ToolKind, ValidationRequest,
    VersionRecommendation,
};

async fn ready_binary(store: &Store) -> vm_packages::SubmissionRecord {
    store
        .register_tool(RegisterTool {
            name: "typemill".into(),
            kind: ToolKind::Binary,
            repository: "https://example.invalid/typemill.git".into(),
            default_branch: "main".into(),
            build_sources: Vec::new(),
            workspace_release: true,
        })
        .await
        .unwrap();
    let checkout = store
        .create_checkout(CreateCheckout {
            package: "typemill".into(),
            agent: "codex".into(),
            consumers: Vec::new(),
            task: "release typemill".into(),
            workspace_release: true,
            source_only: false,
            lease_token: "lease-token-012345678901234567890123456789".into(),
            idempotency_key: "create-binary-build".into(),
        })
        .await
        .unwrap()
        .checkout;
    store
        .record_workspace_source(
            &checkout.checkout_id,
            "main".into(),
            "a".repeat(40),
            "workspace/typemill".into(),
            "/data/agents/typemill/source".into(),
            false,
        )
        .await
        .unwrap();
    store
        .transition(
            &checkout.checkout_id,
            vm_packages::TransitionRequest {
                next: WorkflowState::Active,
                actor: "codex".into(),
                reason: "workspace source ready".into(),
                commit: Some("a".repeat(40)),
                validation_result: None,
                idempotency_key: "activate-binary-build".into(),
            },
        )
        .await
        .unwrap();
    let submission = store
        .record_submission(
            &checkout.checkout_id,
            ImportedSubmission {
                submitted_commit: "b".repeat(40),
                diff_digest: "c".repeat(64),
            },
        )
        .await
        .unwrap();
    store
        .validate_submission(
            &submission.submission_id,
            ValidationRequest {
                package: CheckOutcome::Passed,
                consumers: BTreeMap::new(),
                actor: "controller".into(),
                idempotency_key: "validate-binary-build".into(),
            },
        )
        .await
        .unwrap();
    store
        .record_review(
            &submission.submission_id,
            ReviewRequest {
                decision: ReviewDecision::Approve,
                recommended_version: VersionRecommendation::Patch,
                api_diff: PublicApiDiff {
                    changed_paths: vec!["vm-tool.yaml".into()],
                    potentially_breaking: false,
                },
                reason: "binary source approved".into(),
                required_followups: Vec::new(),
                merge_strategy: "rebase".into(),
                reviewer: "reviewer".into(),
                idempotency_key: "review-binary-build".into(),
            },
        )
        .await
        .unwrap();
    store
        .record_integration(
            &submission.submission_id,
            IntegrationRecord {
                canonical_commit: "a".repeat(40),
                integration_commit: "b".repeat(40),
                strategy: "workspace".into(),
                worktree: "/data/agents/typemill/integration".into(),
                validation: None,
                timestamp: Utc::now(),
            },
            "controller",
            "integrate-binary-build".into(),
        )
        .await
        .unwrap();
    store
        .complete_integration(
            &submission.submission_id,
            ValidationRequest {
                package: CheckOutcome::Passed,
                consumers: BTreeMap::new(),
                actor: "controller".into(),
                idempotency_key: "complete-binary-integration".into(),
            },
        )
        .await
        .unwrap()
}

fn successful_request() -> CompleteToolBuildRequest {
    CompleteToolBuildRequest {
        source_commit: "b".repeat(40),
        manifest_digest: "c".repeat(64),
        version: "1.0.0".into(),
        artifacts: vec![ToolBuildArtifact {
            target: "linux-arm64".into(),
            artifact_digest: "d".repeat(64),
            size_bytes: 42,
            links: BTreeMap::from([(".local/bin/typemill".into(), "bin/typemill".into())]),
        }],
        failure: None,
        failure_kind: None,
        actor: "tool-build-service".into(),
        idempotency_key: "complete-binary-build".into(),
    }
}

#[tokio::test]
async fn binary_release_waits_for_one_durable_successful_build() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let submission = ready_binary(&store).await;
    store
        .update_tool_build_progress(
            &submission.submission_id,
            UpdateToolBuildProgressRequest {
                attempt: "build-attempt-success".into(),
                phase: ToolBuildPhase::Preparing,
                target: None,
                actor: "tool-build-service".into(),
                idempotency_key: "tool-build-progress-success".into(),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        store.next_tool_build().await.unwrap().submission_id,
        submission.submission_id
    );
    assert!(store.next_release().await.is_none());
    let release_request = BeginReleaseRequest {
        version: "1.0.0".into(),
        tag: "v1.0.0".into(),
        source_commit: "b".repeat(40),
        artifact_digest: "d".repeat(64),
        source_pushed: true,
        source_archive_digest: None,
        registry: "http://gateway:8080/tools/typemill".into(),
        expected_publications: Vec::new(),
        actor: "tool-release-service".into(),
        idempotency_key: "begin-binary-release".into(),
    };
    assert!(store
        .begin_release(&submission.submission_id, release_request.clone())
        .await
        .is_err());
    let built = store
        .complete_tool_build(&submission.submission_id, successful_request())
        .await
        .unwrap();
    assert!(built.succeeded());
    let progress = store
        .submission(&submission.submission_id)
        .await
        .unwrap()
        .build_progress
        .unwrap();
    assert_eq!(progress.phase, ToolBuildPhase::Complete);
    assert_eq!(progress.attempt, "build-attempt-success");
    assert!(progress.target.is_none());
    assert!(store.next_tool_build().await.is_none());
    assert_eq!(
        store.next_release().await.unwrap().submission_id,
        submission.submission_id
    );
    assert_eq!(
        store
            .begin_release(&submission.submission_id, release_request)
            .await
            .unwrap()
            .artifact_digest,
        "d".repeat(64)
    );

    drop(store);
    let reopened = Store::open(directory.path()).await.unwrap();
    assert!(reopened
        .tool_build(&submission.submission_id)
        .await
        .unwrap()
        .succeeded());
}

#[tokio::test]
async fn build_progress_is_durable_idempotent_and_monotonic() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let submission = ready_binary(&store).await;
    let preparing = UpdateToolBuildProgressRequest {
        attempt: "build-attempt-1".into(),
        phase: ToolBuildPhase::Preparing,
        target: None,
        actor: "tool-build-service".into(),
        idempotency_key: "tool-build-progress-preparing".into(),
    };

    let first = store
        .update_tool_build_progress(&submission.submission_id, preparing.clone())
        .await
        .unwrap();
    let repeated = store
        .update_tool_build_progress(&submission.submission_id, preparing)
        .await
        .unwrap();
    assert_eq!(first.build_progress, repeated.build_progress);

    store
        .update_tool_build_progress(
            &submission.submission_id,
            UpdateToolBuildProgressRequest {
                attempt: "build-attempt-1".into(),
                phase: ToolBuildPhase::Building,
                target: Some("linux-arm64".into()),
                actor: "tool-build-service".into(),
                idempotency_key: "tool-build-progress-building".into(),
            },
        )
        .await
        .unwrap();
    assert!(store
        .update_tool_build_progress(
            &submission.submission_id,
            UpdateToolBuildProgressRequest {
                attempt: "build-attempt-1".into(),
                phase: ToolBuildPhase::Preparing,
                target: None,
                actor: "tool-build-service".into(),
                idempotency_key: "tool-build-progress-backwards".into(),
            },
        )
        .await
        .is_err());
    store
        .update_tool_build_progress(
            &submission.submission_id,
            UpdateToolBuildProgressRequest {
                attempt: "build-attempt-2".into(),
                phase: ToolBuildPhase::Preparing,
                target: None,
                actor: "tool-build-service".into(),
                idempotency_key: "tool-build-progress-restarted".into(),
            },
        )
        .await
        .unwrap();
    store
        .update_tool_build_progress(
            &submission.submission_id,
            UpdateToolBuildProgressRequest {
                attempt: "build-attempt-2".into(),
                phase: ToolBuildPhase::Building,
                target: Some("linux-arm64".into()),
                actor: "tool-build-service".into(),
                idempotency_key: "tool-build-progress-restarted-building".into(),
            },
        )
        .await
        .unwrap();

    drop(store);
    let reopened = Store::open(directory.path()).await.unwrap();
    let progress = reopened
        .submission(&submission.submission_id)
        .await
        .unwrap()
        .build_progress
        .unwrap();
    assert_eq!(progress.phase, ToolBuildPhase::Building);
    assert_eq!(progress.attempt, "build-attempt-2");
    assert_eq!(progress.target.as_deref(), Some("linux-arm64"));
}

#[tokio::test]
async fn deterministic_build_failure_returns_the_workspace_to_rework() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let submission = ready_binary(&store).await;
    let mut request = successful_request();
    request.artifacts.clear();
    request.failure = Some("binary build failed: test failed".into());
    request.version.clear();
    request.idempotency_key = "failed-binary-build".into();

    let record = store
        .complete_tool_build(&submission.submission_id, request)
        .await
        .unwrap();
    assert!(!record.succeeded());
    let submission = store.submission(&submission.submission_id).await.unwrap();
    assert_eq!(submission.state, WorkflowState::NeedsChanges);
    let progress = submission.build_progress.as_ref().unwrap();
    assert_eq!(progress.phase, ToolBuildPhase::Failed);
    assert!(progress.target.is_none());
    assert_eq!(
        submission.review.as_ref().unwrap().decision,
        ReviewDecision::NeedsChanges
    );
    assert!(store.next_tool_build().await.is_none());
    assert!(store.next_release().await.is_none());
    assert!(submission.review.as_ref().unwrap().required_followups[0].contains("resubmit"));

    let retried = store
        .record_submission(
            &submission.checkout_id,
            ImportedSubmission {
                submitted_commit: submission.submitted_commit.clone(),
                diff_digest: submission.diff_digest.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(retried.state, WorkflowState::Submitted);
    assert!(retried.review.is_none());
}

#[tokio::test]
async fn version_preflight_failure_returns_an_actionable_followup() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let submission = ready_binary(&store).await;
    let mut request = successful_request();
    request.artifacts.clear();
    request.failure = Some("release version 1.1.0 must be newer than 1.1.0".into());
    request.failure_kind = Some(ToolBuildFailureKind::Version);
    request.version.clear();
    request.idempotency_key = "failed-version-preflight".into();

    store
        .complete_tool_build(&submission.submission_id, request)
        .await
        .unwrap();
    let review = store
        .submission(&submission.submission_id)
        .await
        .unwrap()
        .review
        .unwrap();
    assert_eq!(review.decision, ReviewDecision::NeedsChanges);
    assert_eq!(
        review.required_followups,
        ["Update the declared version, commit it, and rerun the same release command"]
    );
}
