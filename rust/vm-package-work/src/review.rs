use chrono::Utc;
use vm_packages::{
    validate_label, IntegrationReview, ReceiptKind, ReviewDecision, ReviewRequest,
    SubmissionRecord, WorkflowState,
};

use crate::store::{
    ensure_fingerprint, operation_fingerprint, validate_idempotency_key, IdempotencyRecord,
};
use crate::workflow::transition_records;
use crate::{Store, WorkError, WorkResult};

impl Store {
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
