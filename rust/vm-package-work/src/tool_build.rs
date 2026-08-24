use std::collections::BTreeSet;

use chrono::Utc;
use vm_packages::{
    validate_label, validate_sha256, CompleteToolBuildRequest, PublishToolArtifact, ReceiptKind,
    ReviewDecision, SourceKind, ToolBuildFailureKind, ToolBuildRecord, WorkflowState,
};

use crate::store::{
    ensure_fingerprint, operation_fingerprint, validate_idempotency_key, IdempotencyRecord, Store,
};
use crate::submission::transition_records;
use crate::{WorkError, WorkResult};

impl Store {
    pub async fn next_tool_build(&self) -> Option<vm_packages::SubmissionRecord> {
        let database = self.database.lock().await;
        database
            .submissions
            .values()
            .filter(|submission| submission.state == WorkflowState::ReadyToRelease)
            .filter(|submission| {
                database
                    .checkouts
                    .get(&submission.checkout_id)
                    .is_some_and(|checkout| checkout.source_kind == SourceKind::ToolBinary)
            })
            .filter(|submission| {
                !database
                    .tool_builds
                    .get(&submission.submission_id)
                    .is_some_and(|build| {
                        submission.integration.as_ref().is_some_and(|integration| {
                            build.source_commit == integration.integration_commit
                                && build.succeeded()
                        })
                    })
            })
            .min_by_key(|submission| submission.updated_at)
            .cloned()
    }

    pub async fn tool_build(&self, submission_id: &str) -> WorkResult<ToolBuildRecord> {
        self.database
            .lock()
            .await
            .tool_builds
            .get(submission_id)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("tool build for {submission_id}")))
    }

    pub async fn complete_tool_build(
        &self,
        submission_id: &str,
        request: CompleteToolBuildRequest,
    ) -> WorkResult<ToolBuildRecord> {
        validate_build_request(&request)?;
        let fingerprint =
            operation_fingerprint("complete_tool_build", Some(submission_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .tool_builds
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }

        let mut next = current.clone();
        let submission = next
            .submissions
            .get(submission_id)
            .ok_or_else(|| WorkError::NotFound(submission_id.to_string()))?;
        if submission.state != WorkflowState::ReadyToRelease || submission.release_id.is_some() {
            return Err(WorkError::Conflict(
                "only an unpublished ready binary release can record a build".into(),
            ));
        }
        let checkout = next
            .checkouts
            .get(&submission.checkout_id)
            .ok_or_else(|| WorkError::Internal("tool build checkout is missing".into()))?;
        if checkout.source_kind != SourceKind::ToolBinary {
            return Err(WorkError::Conflict(
                "only binary tool submissions have a build stage".into(),
            ));
        }
        let expected_commit = submission
            .integration
            .as_ref()
            .ok_or_else(|| WorkError::Conflict("tool build integration is missing".into()))?
            .integration_commit
            .clone();
        if request.source_commit != expected_commit {
            return Err(WorkError::Conflict(
                "tool build source does not match the validated integration".into(),
            ));
        }

        let record = ToolBuildRecord {
            submission_id: submission_id.to_string(),
            source_commit: request.source_commit,
            manifest_digest: request.manifest_digest,
            version: request.version,
            artifacts: request.artifacts,
            failure: request.failure,
            failure_kind: request.failure_kind,
            actor: request.actor.clone(),
            completed_at: Utc::now(),
        };
        next.tool_builds
            .insert(submission_id.to_string(), record.clone());
        if let Some(reason) = &record.failure {
            transition_records(
                &mut next,
                submission_id,
                WorkflowState::NeedsChanges,
                ReceiptKind::Build,
                &request.actor,
                reason,
                Some("needs_changes".into()),
            )?;
            let review = next
                .submissions
                .get_mut(submission_id)
                .and_then(|submission| submission.review.as_mut())
                .ok_or_else(|| WorkError::Conflict("tool build review is missing".into()))?;
            review.decision = ReviewDecision::NeedsChanges;
            review.reason = reason.clone();
            review.required_followups = vec![match record.failure_kind {
                Some(ToolBuildFailureKind::Version) => {
                    "Update the declared version, commit it, and rerun the same release command"
                        .into()
                }
                _ => "Fix the binary build and resubmit".into(),
            }];
            review.reviewer = request.actor.clone();
            review.timestamp = Utc::now();
        }
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: submission_id.to_string(),
            },
        );
        self.commit(&mut current, next).await?;
        Ok(record)
    }
}

fn validate_build_request(request: &CompleteToolBuildRequest) -> WorkResult<()> {
    validate_label("build actor", &request.actor)?;
    validate_idempotency_key(&request.idempotency_key)?;
    validate_sha256(&request.manifest_digest)?;
    if matches!(
        (
            request.failure.as_ref(),
            request.failure_kind,
            request.artifacts.is_empty()
        ),
        (Some(_), _, false) | (None, Some(_), _) | (None, None, true)
    ) {
        return Err(WorkError::Invalid(
            "tool build must contain either artifacts or one failure".into(),
        ));
    }
    if let Some(failure) = &request.failure {
        if failure.trim().is_empty() || failure.len() > 4_000 {
            return Err(WorkError::Invalid(
                "tool build failure must contain 1 to 4000 characters".into(),
            ));
        }
        return Ok(());
    }
    if request.artifacts.len() > 16 {
        return Err(WorkError::Invalid(
            "tool build cannot contain more than 16 artifacts".into(),
        ));
    }
    let mut targets = BTreeSet::new();
    for artifact in &request.artifacts {
        if !targets.insert(&artifact.target) {
            return Err(WorkError::Invalid(
                "tool build artifact targets must be unique".into(),
            ));
        }
        PublishToolArtifact {
            version: request.version.clone(),
            target: artifact.target.clone(),
            artifact_digest: artifact.artifact_digest.clone(),
            size_bytes: artifact.size_bytes,
            links: artifact.links.clone(),
            source_commit: request.source_commit.clone(),
            tag: format!("v{}", request.version),
            actor: request.actor.clone(),
            idempotency_key: request.idempotency_key.clone(),
        }
        .validate()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
        assert_eq!(
            submission.review.unwrap().decision,
            ReviewDecision::NeedsChanges
        );
        assert!(store.next_tool_build().await.is_none());
        assert!(store.next_release().await.is_none());

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
        assert!(store.tool_build(&retried.submission_id).await.is_err());
        let revalidated = store
            .validate_submission(
                &retried.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::new(),
                    actor: "controller".into(),
                    idempotency_key: "validate-binary-build".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(revalidated.state, WorkflowState::Reviewing);
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
}
