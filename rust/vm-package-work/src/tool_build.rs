use std::collections::BTreeSet;

use chrono::Utc;
use vm_packages::{
    validate_label, validate_sha256, CompleteToolBuildRequest, PublishToolArtifact, ReceiptKind,
    ReviewDecision, SourceKind, SubmissionRecord, ToolBuildFailureKind, ToolBuildPhase,
    ToolBuildProgress, ToolBuildRecord, UpdateToolBuildProgressRequest, WorkflowState,
};

use crate::store::{
    ensure_fingerprint, operation_fingerprint, validate_idempotency_key, IdempotencyRecord, Store,
};
use crate::workflow::transition_records;
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

    pub async fn update_tool_build_progress(
        &self,
        submission_id: &str,
        request: UpdateToolBuildProgressRequest,
    ) -> WorkResult<SubmissionRecord> {
        request.validate()?;
        validate_idempotency_key(&request.idempotency_key)?;
        let fingerprint =
            operation_fingerprint("update_tool_build_progress", Some(submission_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .submissions
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("build progress target is missing".into()));
        }

        let mut next = current.clone();
        let submission = next
            .submissions
            .get_mut(submission_id)
            .ok_or_else(|| WorkError::NotFound(submission_id.to_string()))?;
        if submission.state != WorkflowState::ReadyToRelease || submission.release_id.is_some() {
            return Err(WorkError::Conflict(
                "build progress requires an unpublished ready binary release".into(),
            ));
        }
        let checkout = next
            .checkouts
            .get(&submission.checkout_id)
            .ok_or_else(|| WorkError::Internal("tool build checkout is missing".into()))?;
        if checkout.source_kind != SourceKind::ToolBinary {
            return Err(WorkError::Conflict(
                "only binary tool submissions have build progress".into(),
            ));
        }
        let same_attempt = submission
            .build_progress
            .as_ref()
            .is_some_and(|progress| progress.attempt == request.attempt);
        if !same_attempt && request.phase != ToolBuildPhase::Preparing {
            return Err(WorkError::Conflict(
                "a new tool build attempt must begin with preparing progress".into(),
            ));
        }
        if same_attempt
            && submission
                .build_progress
                .as_ref()
                .is_some_and(|progress| request.phase < progress.phase)
        {
            return Err(WorkError::Conflict(
                "tool build progress cannot move backwards".into(),
            ));
        }
        let now = Utc::now();
        let started_at = if same_attempt {
            submission
                .build_progress
                .as_ref()
                .map_or(now, |progress| progress.started_at)
        } else {
            now
        };
        submission.build_progress = Some(ToolBuildProgress {
            attempt: request.attempt,
            phase: request.phase,
            target: request.target,
            actor: request.actor,
            started_at,
            updated_at: now,
        });
        submission.updated_at = now;
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: submission_id.to_string(),
            },
        );
        let result = submission.clone();
        self.commit(&mut current, next).await?;
        Ok(result)
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
            completion_idempotency_key: Some(request.idempotency_key.clone()),
            completed_at: Utc::now(),
        };
        next.tool_builds
            .insert(submission_id.to_string(), record.clone());
        let now = Utc::now();
        let submission = next
            .submissions
            .get_mut(submission_id)
            .expect("tool build submission remains present");
        let started_at = submission
            .build_progress
            .as_ref()
            .map_or(now, |progress| progress.started_at);
        submission.build_progress = Some(ToolBuildProgress {
            attempt: submission
                .build_progress
                .as_ref()
                .map_or_else(|| "completion".into(), |progress| progress.attempt.clone()),
            phase: if record.failure.is_some() {
                ToolBuildPhase::Failed
            } else {
                ToolBuildPhase::Complete
            },
            target: None,
            actor: request.actor.clone(),
            started_at,
            updated_at: now,
        });
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
                _ => {
                    "Fix the reported binary-build problem, commit the repair, and resubmit".into()
                }
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

mod validation;
use validation::validate_build_request;

#[cfg(test)]
mod tests;
