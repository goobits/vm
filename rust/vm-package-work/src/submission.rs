use chrono::Utc;
use vm_packages::{
    validate_label, IntegrationRecord, IntegrationReview, ReceiptKind, ReviewDecision,
    ReviewRequest, SubmissionRecord, ValidationRequest, ValidationResult, WorkflowState,
    WorkflowTransition,
};

use crate::store::{
    ensure_fingerprint, operation_fingerprint, receipt, validate_idempotency_key,
    IdempotencyRecord, ReceiptInput,
};
use crate::{Store, WorkError, WorkResult};

pub struct ImportedSubmission {
    pub submitted_commit: String,
    pub diff_digest: String,
}

impl Store {
    pub async fn record_submission(
        &self,
        checkout_id: &str,
        imported: ImportedSubmission,
    ) -> WorkResult<SubmissionRecord> {
        let submission_id = format!(
            "sub-{checkout_id}-{}",
            imported
                .submitted_commit
                .chars()
                .take(12)
                .collect::<String>()
        );
        let mut current = self.database.lock().await;
        if let Some(existing) = current.submissions.get(&submission_id) {
            if existing.diff_digest == imported.diff_digest {
                return Ok(existing.clone());
            }
            return Err(WorkError::Conflict(
                "submission commit was already recorded with a different diff".into(),
            ));
        }
        let mut next = current.clone();
        let now = Utc::now();
        let checkout = next
            .checkouts
            .get(checkout_id)
            .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?;
        if checkout.state != WorkflowState::Active {
            return Err(WorkError::Conflict(
                "only an active checkout can be submitted".into(),
            ));
        }
        let branch = checkout
            .branch
            .clone()
            .ok_or_else(|| WorkError::Conflict("checkout branch is missing".into()))?;
        let base_commit = checkout
            .base_commit
            .clone()
            .ok_or_else(|| WorkError::Conflict("checkout base commit is missing".into()))?;
        let record = SubmissionRecord {
            submission_id: submission_id.clone(),
            checkout_id: checkout_id.to_string(),
            package: checkout.package.clone(),
            branch,
            base_commit,
            submitted_commit: imported.submitted_commit,
            diff_digest: imported.diff_digest,
            state: WorkflowState::Active,
            validation: None,
            review: None,
            integration: None,
            release_id: None,
            created_at: now,
            updated_at: now,
        };
        next.submissions.insert(submission_id.clone(), record);
        transition_records(
            &mut next,
            &submission_id,
            WorkflowState::Submitted,
            ReceiptKind::Submission,
            "package-agent",
            "committed package changes submitted",
            Some("bundle_verified".into()),
        )?;
        let result = next
            .submissions
            .get(&submission_id)
            .cloned()
            .expect("submission remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn submission(&self, submission_id: &str) -> WorkResult<SubmissionRecord> {
        self.database
            .lock()
            .await
            .submissions
            .get(submission_id)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(submission_id.to_string()))
    }

    pub async fn submissions(&self) -> Vec<SubmissionRecord> {
        self.database
            .lock()
            .await
            .submissions
            .values()
            .cloned()
            .collect()
    }

    pub async fn checkout_submission(&self, checkout_id: &str) -> WorkResult<SubmissionRecord> {
        self.database
            .lock()
            .await
            .submissions
            .values()
            .filter(|submission| submission.checkout_id == checkout_id)
            .max_by_key(|submission| submission.created_at)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("submission for {checkout_id}")))
    }

    pub async fn validate_submission(
        &self,
        submission_id: &str,
        request: ValidationRequest,
    ) -> WorkResult<SubmissionRecord> {
        validate_validation(&request)?;
        let fingerprint =
            operation_fingerprint("validate_submission", Some(submission_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .submissions
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let mut next = current.clone();
        let checkout_id = next
            .submissions
            .get(submission_id)
            .ok_or_else(|| WorkError::NotFound(submission_id.to_string()))?
            .checkout_id
            .clone();
        let expected_consumers = &next
            .checkouts
            .get(&checkout_id)
            .ok_or_else(|| WorkError::Internal("submission checkout is missing".into()))?
            .consumers;
        if request.consumers.keys().ne(expected_consumers.iter()) {
            return Err(WorkError::Invalid(
                "validation must report every selected consumer exactly once".into(),
            ));
        }
        let validation = ValidationResult {
            package: request.package,
            consumers: request.consumers,
            actor: request.actor.clone(),
            timestamp: Utc::now(),
        };
        transition_records(
            &mut next,
            submission_id,
            WorkflowState::Validating,
            ReceiptKind::Validation,
            &request.actor,
            "deterministic validation recorded",
            Some(
                if validation.passed() {
                    "passed"
                } else {
                    "failed"
                }
                .into(),
            ),
        )?;
        next.submissions
            .get_mut(submission_id)
            .expect("submission remains present")
            .validation = Some(validation.clone());
        let target = if validation.passed() {
            WorkflowState::Reviewing
        } else {
            WorkflowState::Failed
        };
        transition_records(
            &mut next,
            submission_id,
            target,
            ReceiptKind::Validation,
            &request.actor,
            if validation.passed() {
                "validation passed; integration review requested"
            } else {
                "validation failed"
            },
            Some(
                if validation.passed() {
                    "passed"
                } else {
                    "failed"
                }
                .into(),
            ),
        )?;
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: submission_id.to_string(),
            },
        );
        let result = next
            .submissions
            .get(submission_id)
            .cloned()
            .expect("submission remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn record_review(
        &self,
        submission_id: &str,
        request: ReviewRequest,
    ) -> WorkResult<SubmissionRecord> {
        validate_review(&request)?;
        let fingerprint = operation_fingerprint("review", Some(submission_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .submissions
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let mut next = current.clone();
        let review = IntegrationReview {
            decision: request.decision,
            recommended_version: request.recommended_version,
            api_diff: request.api_diff,
            reason: request.reason,
            required_followups: request.required_followups,
            merge_strategy: request.merge_strategy,
            reviewer: request.reviewer,
            timestamp: Utc::now(),
        };
        let target = match review.decision {
            ReviewDecision::Approve => WorkflowState::Approved,
            ReviewDecision::Reject => WorkflowState::Rejected,
            ReviewDecision::NeedsChanges => WorkflowState::NeedsChanges,
        };
        transition_records(
            &mut next,
            submission_id,
            target,
            ReceiptKind::Review,
            &review.reviewer,
            &review.reason,
            Some(format!("{:?}", review.decision).to_ascii_lowercase()),
        )?;
        next.submissions
            .get_mut(submission_id)
            .expect("submission remains present")
            .review = Some(review);
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: submission_id.to_string(),
            },
        );
        let result = next
            .submissions
            .get(submission_id)
            .cloned()
            .expect("submission remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn record_integration(
        &self,
        submission_id: &str,
        integration: IntegrationRecord,
        actor: &str,
        idempotency_key: String,
    ) -> WorkResult<SubmissionRecord> {
        validate_label("integration actor", actor)?;
        validate_idempotency_key(&idempotency_key)?;
        let fingerprint =
            operation_fingerprint("integrate", Some(submission_id), &(&integration, actor))?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .submissions
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let mut next = current.clone();
        transition_records(
            &mut next,
            submission_id,
            WorkflowState::Integrating,
            ReceiptKind::Integration,
            actor,
            "approved submission integrated with current canonical source",
            Some("integration_prepared".into()),
        )?;
        next.submissions
            .get_mut(submission_id)
            .expect("submission remains present")
            .integration = Some(integration);
        next.idempotency.insert(
            idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: submission_id.to_string(),
            },
        );
        let result = next
            .submissions
            .get(submission_id)
            .cloned()
            .expect("submission remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn complete_integration(
        &self,
        submission_id: &str,
        request: ValidationRequest,
    ) -> WorkResult<SubmissionRecord> {
        validate_validation(&request)?;
        let fingerprint =
            operation_fingerprint("complete_integration", Some(submission_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .submissions
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let mut next = current.clone();
        let validation = ValidationResult {
            package: request.package,
            consumers: request.consumers,
            actor: request.actor.clone(),
            timestamp: Utc::now(),
        };
        let target = if validation.passed() {
            WorkflowState::ReadyToRelease
        } else {
            WorkflowState::Failed
        };
        transition_records(
            &mut next,
            submission_id,
            target,
            ReceiptKind::Integration,
            &request.actor,
            if validation.passed() {
                "integrated package and consumer checks passed"
            } else {
                "integrated package or consumer checks failed"
            },
            Some(
                if validation.passed() {
                    "passed"
                } else {
                    "failed"
                }
                .into(),
            ),
        )?;
        next.submissions
            .get_mut(submission_id)
            .and_then(|submission| submission.integration.as_mut())
            .ok_or_else(|| WorkError::Conflict("integration record is missing".into()))?
            .validation = Some(validation);
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: submission_id.to_string(),
            },
        );
        let result = next
            .submissions
            .get(submission_id)
            .cloned()
            .expect("submission remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }
}

pub(crate) fn transition_records(
    database: &mut crate::store::Database,
    submission_id: &str,
    next: WorkflowState,
    kind: ReceiptKind,
    actor: &str,
    reason: &str,
    validation_result: Option<String>,
) -> WorkResult<()> {
    let submission = database
        .submissions
        .get(submission_id)
        .ok_or_else(|| WorkError::NotFound(submission_id.to_string()))?;
    let previous = submission.state;
    if !previous.can_transition_to(next) {
        return Err(WorkError::Conflict(format!(
            "cannot transition submission from {previous:?} to {next:?}"
        )));
    }
    let checkout_id = submission.checkout_id.clone();
    let commit = Some(submission.integration.as_ref().map_or_else(
        || submission.submitted_commit.clone(),
        |integration| integration.integration_commit.clone(),
    ));
    let now = Utc::now();
    let workflow_receipt = receipt(
        database,
        ReceiptInput {
            kind,
            checkout_id: &checkout_id,
            actor,
            previous: Some(previous),
            next,
            commit: commit.clone(),
            validation_result: validation_result.clone(),
            reason,
            timestamp: now,
        },
    );
    let submission = database
        .submissions
        .get_mut(submission_id)
        .expect("submission remains present");
    submission.state = next;
    submission.updated_at = now;
    let checkout = database
        .checkouts
        .get_mut(&checkout_id)
        .ok_or_else(|| WorkError::Internal("submission checkout is missing".into()))?;
    if checkout.state != previous {
        return Err(WorkError::Conflict(
            "checkout and submission workflow states diverged".into(),
        ));
    }
    checkout.state = next;
    checkout.updated_at = now;
    if next.revokes_lease() {
        checkout.lease = None;
    }
    checkout.transitions.push(WorkflowTransition {
        timestamp: now,
        actor: actor.to_string(),
        previous: Some(previous),
        next,
        commit,
        validation_result,
        reason: reason.to_string(),
        receipt_id: workflow_receipt.receipt_id.clone(),
    });
    database
        .receipts
        .insert(workflow_receipt.receipt_id.clone(), workflow_receipt);
    Ok(())
}

fn validate_validation(request: &ValidationRequest) -> WorkResult<()> {
    validate_label("validation actor", &request.actor)?;
    for consumer in request.consumers.keys() {
        validate_label("consumer", consumer)?;
    }
    validate_idempotency_key(&request.idempotency_key)
}

fn validate_review(request: &ReviewRequest) -> WorkResult<()> {
    validate_label("reviewer", &request.reviewer)?;
    if request.reason.trim().is_empty() || request.reason.len() > 4_000 {
        return Err(WorkError::Invalid(
            "review reason must contain 1 to 4000 characters".into(),
        ));
    }
    if !matches!(request.merge_strategy.as_str(), "rebase" | "merge") {
        return Err(WorkError::Invalid(
            "merge strategy must be rebase or merge".into(),
        ));
    }
    validate_idempotency_key(&request.idempotency_key)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use vm_packages::{CheckOutcome, CreateCheckout};

    #[tokio::test]
    async fn submission_validation_and_review_are_durable_and_ordered() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        let created = store
            .create_checkout(CreateCheckout {
                package: "auth".into(),
                agent: "agent-1".into(),
                consumers: vec!["project-a".into()],
                task: "change auth".into(),
                idempotency_key: "create".into(),
            })
            .await
            .unwrap();
        store
            .record_source(
                &created.checkout.checkout_id,
                "main".into(),
                "abc123".into(),
                "agents/one".into(),
                "/data/agents/one".into(),
            )
            .await
            .unwrap();
        store
            .transition(
                &created.checkout.checkout_id,
                vm_packages::TransitionRequest {
                    next: WorkflowState::Active,
                    actor: "agent-1".into(),
                    reason: "attached".into(),
                    commit: Some("abc123".into()),
                    validation_result: None,
                    idempotency_key: "active".into(),
                },
            )
            .await
            .unwrap();
        let submission = store
            .record_submission(
                &created.checkout.checkout_id,
                ImportedSubmission {
                    submitted_commit: "def456789012345".into(),
                    diff_digest: "digest".into(),
                },
            )
            .await
            .unwrap();
        let validated = store
            .validate_submission(
                &submission.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::from([("project-a".into(), CheckOutcome::Passed)]),
                    actor: "controller".into(),
                    idempotency_key: "validate".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(validated.state, WorkflowState::Reviewing);
        let reviewed = store
            .record_review(
                &submission.submission_id,
                ReviewRequest {
                    decision: ReviewDecision::Approve,
                    recommended_version: vm_packages::VersionRecommendation::Patch,
                    api_diff: vm_packages::PublicApiDiff {
                        changed_paths: vec![],
                        potentially_breaking: false,
                    },
                    reason: "compatible change".into(),
                    required_followups: vec![],
                    merge_strategy: "rebase".into(),
                    reviewer: "integration-agent".into(),
                    idempotency_key: "review".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(reviewed.state, WorkflowState::Approved);
    }
}
