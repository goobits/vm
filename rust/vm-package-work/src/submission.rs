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

pub(crate) struct ImportedSubmission {
    pub submitted_commit: String,
    pub diff_digest: String,
}

impl Store {
    pub async fn record_submission(
        &self,
        checkout_id: &str,
        imported: ImportedSubmission,
    ) -> WorkResult<SubmissionRecord> {
        let submission_id = format!("sub-{checkout_id}");
        let mut current = self.database.lock().await;
        let existing_id = current
            .submissions
            .values()
            .filter(|submission| submission.checkout_id == checkout_id)
            .max_by_key(|submission| submission.created_at)
            .map(|submission| submission.submission_id.clone());
        if let Some(existing) = existing_id
            .as_ref()
            .and_then(|id| current.submissions.get(id))
            .cloned()
        {
            let retry_failed_tool_build = existing.state == WorkflowState::NeedsChanges
                && existing
                    .review
                    .as_ref()
                    .is_some_and(|review| review.reviewer == "tool-build-service");
            if existing.diff_digest == imported.diff_digest && !retry_failed_tool_build {
                if existing.state == WorkflowState::Submitted && existing.validation.is_none() {
                    let mut next = current.clone();
                    next.idempotency
                        .retain(|_, record| record.target_id != existing.submission_id);
                    self.commit(&mut current, next).await?;
                }
                return Ok(existing);
            }
            if existing.state != WorkflowState::NeedsChanges {
                return Err(WorkError::Conflict(
                    "submission can only be replaced after review requests changes".into(),
                ));
            }
        }
        let mut next = current.clone();
        let now = Utc::now();
        let checkout = next
            .checkouts
            .get(checkout_id)
            .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?;
        if !matches!(
            checkout.state,
            WorkflowState::Active | WorkflowState::NeedsChanges
        ) {
            return Err(WorkError::Conflict(
                "only an active checkout or requested rework can be submitted".into(),
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
        let submission_id = existing_id.unwrap_or(submission_id);
        if next.submissions.contains_key(&submission_id) {
            transition_records(
                &mut next,
                &submission_id,
                WorkflowState::Active,
                ReceiptKind::Submission,
                "package-agent",
                "review feedback addressed; submission reactivated",
                Some("rework_complete".into()),
            )?;
            let record = next
                .submissions
                .get_mut(&submission_id)
                .expect("submission remains present");
            record.submitted_commit = imported.submitted_commit;
            record.diff_digest = imported.diff_digest;
            record.validation = None;
            record.review = None;
            record.integration = None;
            record.release_id = None;
            record.updated_at = now;
            next.tool_builds.remove(&submission_id);
            next.idempotency
                .retain(|_, record| record.target_id != submission_id);
        } else {
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
        }
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

    pub async fn next_review(&self) -> Option<SubmissionRecord> {
        self.database
            .lock()
            .await
            .submissions
            .values()
            .filter(|submission| submission.state == WorkflowState::Reviewing)
            .min_by_key(|submission| submission.updated_at)
            .cloned()
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
        let mut retry_stale_generation = false;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            let recorded = current
                .submissions
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
            if !recorded.as_ref().is_ok_and(|submission| {
                submission.state == WorkflowState::Submitted && submission.validation.is_none()
            }) {
                return recorded;
            }
            retry_stale_generation = true;
        }
        let mut next = current.clone();
        if retry_stale_generation {
            next.idempotency
                .retain(|_, record| record.target_id != submission_id);
        }
        let checkout_id = next
            .submissions
            .get(submission_id)
            .ok_or_else(|| WorkError::NotFound(submission_id.to_string()))?
            .checkout_id
            .clone();
        let checkout = next
            .checkouts
            .get(&checkout_id)
            .ok_or_else(|| WorkError::Internal("submission checkout is missing".into()))?;
        validate_consumer_results(checkout, &request.consumers)?;
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
        let mut retry_stale_generation = false;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            let recorded = current
                .submissions
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
            if !recorded.as_ref().is_ok_and(|submission| {
                submission.state == WorkflowState::Reviewing && submission.review.is_none()
            }) {
                return recorded;
            }
            retry_stale_generation = true;
        }
        let mut next = current.clone();
        if retry_stale_generation {
            next.idempotency
                .retain(|_, record| record.target_id != submission_id);
        }
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
        let checkout_id = next
            .submissions
            .get(submission_id)
            .ok_or_else(|| WorkError::NotFound(submission_id.to_string()))?
            .checkout_id
            .clone();
        let checkout = next
            .checkouts
            .get(&checkout_id)
            .ok_or_else(|| WorkError::Internal("submission checkout is missing".into()))?;
        validate_consumer_results(checkout, &request.consumers)?;
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
    if next.revokes_lease() {
        database.lease_credentials.remove(&checkout_id);
    }
    Ok(())
}

fn validate_validation(request: &ValidationRequest) -> WorkResult<()> {
    validate_label("validation actor", &request.actor)?;
    for consumer in request.consumers.keys() {
        validate_label("consumer", consumer)?;
    }
    validate_idempotency_key(&request.idempotency_key)
}

fn validate_consumer_results(
    checkout: &vm_packages::CheckoutRecord,
    consumers: &std::collections::BTreeMap<String, vm_packages::CheckOutcome>,
) -> WorkResult<()> {
    let matches_source = match checkout.source_kind {
        vm_packages::SourceKind::Package if !checkout.workspace_release => {
            if checkout.source_only {
                consumers.is_empty()
            } else {
                consumers.keys().eq(checkout.consumers.iter())
            }
        }
        vm_packages::SourceKind::Package
        | vm_packages::SourceKind::ToolBinary
        | vm_packages::SourceKind::ToolCollection => consumers.is_empty(),
    };
    if matches_source {
        Ok(())
    } else {
        Err(WorkError::Invalid(
            "validation consumer results do not match the source kind".into(),
        ))
    }
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
    use vm_packages::{CheckOutcome, CreateCheckout, RegisterTool, ToolKind};

    #[tokio::test]
    async fn collection_validation_has_no_package_consumer_result() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        store
            .register_tool(RegisterTool {
                name: "agent-skills".into(),
                kind: ToolKind::Collection,
                repository: "https://example.invalid/agent-skills.git".into(),
                default_branch: "main".into(),
                workspace_release: false,
            })
            .await
            .unwrap();
        let created = store
            .create_checkout(CreateCheckout {
                package: "agent-skills".into(),
                agent: "agent-1".into(),
                consumers: vec!["project-a".into()],
                task: "change skills".into(),
                workspace_release: false,
                source_only: false,
                lease_token: "lease-token-012345678901234567890123456789".into(),
                idempotency_key: "create-collection".into(),
            })
            .await
            .unwrap();
        store
            .record_source(
                &created.checkout.checkout_id,
                "main".into(),
                "abc123".into(),
                "agents/collection".into(),
                "/data/agents/collection".into(),
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
                    idempotency_key: "activate-collection".into(),
                },
            )
            .await
            .unwrap();
        let submission = store
            .record_submission(
                &created.checkout.checkout_id,
                ImportedSubmission {
                    submitted_commit: "def456789012345".into(),
                    diff_digest: "collection-digest".into(),
                },
            )
            .await
            .unwrap();

        let invalid = store
            .validate_submission(
                &submission.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::from([("project-a".into(), CheckOutcome::Passed)]),
                    actor: "controller".into(),
                    idempotency_key: "validate-collection-invalid".into(),
                },
            )
            .await;
        assert!(invalid.is_err());

        let validated = store
            .validate_submission(
                &submission.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::new(),
                    actor: "controller".into(),
                    idempotency_key: "validate-collection".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(validated.state, WorkflowState::Reviewing);
        assert!(validated.validation.unwrap().consumers.is_empty());
    }

    #[tokio::test]
    async fn source_only_package_uses_no_consumer_result_through_integration() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        let created = store
            .create_checkout(CreateCheckout {
                package: "auth".into(),
                agent: "agent-1".into(),
                consumers: vec!["project-a".into()],
                task: "maintain a package not consumed by this project".into(),
                workspace_release: false,
                source_only: true,
                lease_token: "lease-token-012345678901234567890123456789".into(),
                idempotency_key: "create-source-only".into(),
            })
            .await
            .unwrap();
        store
            .record_source(
                &created.checkout.checkout_id,
                "main".into(),
                "abc123".into(),
                "agents/source-only".into(),
                "/data/agents/source-only".into(),
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
                    idempotency_key: "activate-source-only".into(),
                },
            )
            .await
            .unwrap();
        let submission = store
            .record_submission(
                &created.checkout.checkout_id,
                ImportedSubmission {
                    submitted_commit: "def456789012345".into(),
                    diff_digest: "source-only-digest".into(),
                },
            )
            .await
            .unwrap();

        let invalid = store
            .validate_submission(
                &submission.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::from([("project-a".into(), CheckOutcome::Passed)]),
                    actor: "agent-1".into(),
                    idempotency_key: "validate-source-only-invalid".into(),
                },
            )
            .await;
        assert!(invalid.is_err());
        store
            .validate_submission(
                &submission.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::new(),
                    actor: "agent-1".into(),
                    idempotency_key: "validate-source-only".into(),
                },
            )
            .await
            .unwrap();
        store
            .record_review(
                &submission.submission_id,
                ReviewRequest {
                    decision: ReviewDecision::Approve,
                    recommended_version: vm_packages::VersionRecommendation::Patch,
                    api_diff: vm_packages::PublicApiDiff {
                        changed_paths: vec![],
                        potentially_breaking: false,
                    },
                    reason: "source-only package checks passed".into(),
                    required_followups: vec![],
                    merge_strategy: "rebase".into(),
                    reviewer: "reviewer".into(),
                    idempotency_key: "review-source-only".into(),
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
                    strategy: "rebase".into(),
                    worktree: "/data/agents/source-only/integration".into(),
                    validation: None,
                    timestamp: Utc::now(),
                },
                "agent-1",
                "integrate-source-only".into(),
            )
            .await
            .unwrap();
        let ready = store
            .complete_integration(
                &submission.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::new(),
                    actor: "agent-1".into(),
                    idempotency_key: "complete-source-only".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(ready.state, WorkflowState::ReadyToRelease);
    }

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
                workspace_release: false,
                source_only: false,
                lease_token: "lease-token-012345678901234567890123456789".into(),
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
        let validation_request = ValidationRequest {
            package: CheckOutcome::Passed,
            consumers: BTreeMap::from([("project-a".into(), CheckOutcome::Passed)]),
            actor: "controller".into(),
            idempotency_key: "validate".into(),
        };
        store.database.lock().await.idempotency.insert(
            "validate".into(),
            IdempotencyRecord {
                fingerprint: operation_fingerprint(
                    "validate_submission",
                    Some(&submission.submission_id),
                    &validation_request,
                )
                .unwrap(),
                target_id: submission.submission_id.clone(),
            },
        );
        let validated = store
            .validate_submission(&submission.submission_id, validation_request)
            .await
            .unwrap();
        assert_eq!(validated.state, WorkflowState::Reviewing);
        assert_eq!(
            store.next_review().await.unwrap().submission_id,
            submission.submission_id
        );
        let review_request = ReviewRequest {
            decision: ReviewDecision::NeedsChanges,
            recommended_version: vm_packages::VersionRecommendation::Patch,
            api_diff: vm_packages::PublicApiDiff {
                changed_paths: vec![],
                potentially_breaking: false,
            },
            reason: "add a regression test".into(),
            required_followups: vec!["cover refresh failure".into()],
            merge_strategy: "rebase".into(),
            reviewer: "integration-agent".into(),
            idempotency_key: "review-needs-changes".into(),
        };
        store.database.lock().await.idempotency.insert(
            review_request.idempotency_key.clone(),
            IdempotencyRecord {
                fingerprint: operation_fingerprint(
                    "review",
                    Some(&submission.submission_id),
                    &review_request,
                )
                .unwrap(),
                target_id: submission.submission_id.clone(),
            },
        );
        let changes_requested = store
            .record_review(&submission.submission_id, review_request)
            .await
            .unwrap();
        assert_eq!(changes_requested.state, WorkflowState::NeedsChanges);
        assert!(store.next_review().await.is_none());

        let resubmitted = store
            .record_submission(
                &created.checkout.checkout_id,
                ImportedSubmission {
                    submitted_commit: "fedcba987654321".into(),
                    diff_digest: "updated-digest".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(resubmitted.submission_id, submission.submission_id);
        assert_eq!(resubmitted.state, WorkflowState::Submitted);
        assert!(resubmitted.review.is_none());

        store
            .validate_submission(
                &submission.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::from([("project-a".into(), CheckOutcome::Passed)]),
                    actor: "controller".into(),
                    idempotency_key: "validate-resubmission".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            store.next_review().await.unwrap().submission_id,
            submission.submission_id
        );
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
                    idempotency_key: "review-approved".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(reviewed.state, WorkflowState::Approved);
        assert!(store.next_review().await.is_none());
    }
}
