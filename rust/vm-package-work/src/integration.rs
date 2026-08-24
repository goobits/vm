use chrono::Utc;
use vm_packages::{
    validate_label, IntegrationRecord, ReceiptKind, SubmissionRecord, ValidationRequest,
    ValidationResult, WorkflowState,
};

use crate::store::{
    ensure_fingerprint, operation_fingerprint, validate_idempotency_key, IdempotencyRecord,
};
use crate::submission::{validate_consumer_results, validate_validation};
use crate::workflow::transition_records;
use crate::{Store, WorkError, WorkResult};

impl Store {
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
