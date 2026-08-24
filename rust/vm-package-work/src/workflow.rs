use chrono::Utc;
use vm_packages::{
    validate_label, CheckoutRecord, CleanupRequest, ReceiptKind, TransitionRequest, WorkflowState,
    WorkflowTransition,
};

use crate::store::{
    ensure_fingerprint, operation_fingerprint, receipt, validate_idempotency_key, Database,
    IdempotencyRecord, ReceiptInput,
};
use crate::{Store, WorkError, WorkResult};

pub(crate) fn transition_record(
    database: &mut Database,
    checkout_id: &str,
    request: &TransitionRequest,
) -> WorkResult<()> {
    let previous = database
        .checkouts
        .get(checkout_id)
        .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?
        .state;
    let now = Utc::now();
    let transition = receipt(
        database,
        ReceiptInput {
            kind: ReceiptKind::Transition,
            checkout_id,
            actor: &request.actor,
            previous: Some(previous),
            next: request.next,
            commit: request.commit.clone(),
            validation_result: request.validation_result.clone(),
            reason: &request.reason,
            timestamp: now,
        },
    );
    let checkout = database
        .checkouts
        .get_mut(checkout_id)
        .expect("checkout remains present");
    checkout.state = request.next;
    checkout.updated_at = now;
    if request.next.revokes_lease() {
        checkout.lease = None;
    }
    checkout.transitions.push(WorkflowTransition {
        timestamp: now,
        actor: request.actor.clone(),
        previous: Some(previous),
        next: request.next,
        commit: request.commit.clone(),
        validation_result: request.validation_result.clone(),
        reason: request.reason.clone(),
        receipt_id: transition.receipt_id.clone(),
    });
    database
        .receipts
        .insert(transition.receipt_id.clone(), transition);
    if request.next.revokes_lease() {
        database.lease_credentials.remove(checkout_id);
    }
    Ok(())
}

pub(crate) fn close_record(
    database: &mut Database,
    checkout_id: &str,
    actor: &str,
) -> WorkResult<()> {
    let checkout = database
        .checkouts
        .get(checkout_id)
        .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?;
    let (previous, commit) = (checkout.state, checkout.base_commit.clone());
    let now = Utc::now();
    let cleanup = receipt(
        database,
        ReceiptInput {
            kind: ReceiptKind::Cleanup,
            checkout_id,
            actor,
            previous: Some(previous),
            next: WorkflowState::Closed,
            commit: commit.clone(),
            validation_result: Some("cleanup_complete".into()),
            reason: "temporary checkout data removed",
            timestamp: now,
        },
    );
    let checkout = database
        .checkouts
        .get_mut(checkout_id)
        .expect("checkout remains present");
    checkout.state = WorkflowState::Closed;
    checkout.updated_at = now;
    checkout.transitions.push(WorkflowTransition {
        timestamp: now,
        actor: actor.into(),
        previous: Some(previous),
        next: WorkflowState::Closed,
        commit,
        validation_result: Some("cleanup_complete".into()),
        reason: "temporary checkout data removed".into(),
        receipt_id: cleanup.receipt_id.clone(),
    });
    database
        .receipts
        .insert(cleanup.receipt_id.clone(), cleanup);
    database.lease_credentials.remove(checkout_id);
    Ok(())
}

pub(crate) fn validate_transition(request: &TransitionRequest) -> WorkResult<()> {
    validate_label("actor", &request.actor)?;
    if request.reason.trim().is_empty() || request.reason.len() > 1_000 {
        return Err(WorkError::Invalid(
            "transition reason must contain 1 to 1000 characters".into(),
        ));
    }
    crate::store::validate_idempotency_key(&request.idempotency_key)
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

impl Store {
    pub async fn transition(
        &self,
        checkout_id: &str,
        request: TransitionRequest,
    ) -> WorkResult<CheckoutRecord> {
        validate_transition(&request)?;
        let fingerprint = operation_fingerprint("transition", Some(checkout_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .checkouts
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let mut next = current.clone();
        let previous = next
            .checkouts
            .get(checkout_id)
            .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?
            .state;
        if !previous.can_transition_to(request.next) {
            return Err(WorkError::Conflict(format!(
                "cannot transition from {previous:?} to {:?}",
                request.next
            )));
        }
        let submission_id = next
            .submissions
            .values()
            .filter(|submission| submission.checkout_id == checkout_id)
            .max_by_key(|submission| submission.created_at)
            .map(|submission| submission.submission_id.clone());
        if let Some(submission_id) = submission_id {
            transition_records(
                &mut next,
                &submission_id,
                request.next,
                ReceiptKind::Transition,
                &request.actor,
                &request.reason,
                request.validation_result.clone(),
            )?;
        } else {
            transition_record(&mut next, checkout_id, &request)?;
        }
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: checkout_id.to_string(),
            },
        );
        let result = next
            .checkouts
            .get(checkout_id)
            .cloned()
            .expect("checkout remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn close_checkout(
        &self,
        checkout_id: &str,
        request: CleanupRequest,
    ) -> WorkResult<CheckoutRecord> {
        validate_label("cleanup actor", &request.actor)?;
        validate_idempotency_key(&request.idempotency_key)?;
        let fingerprint = operation_fingerprint("cleanup_checkout", Some(checkout_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .checkouts
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let checkout = current
            .checkouts
            .get(checkout_id)
            .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?;
        if checkout.state == WorkflowState::Closed {
            return Ok(checkout.clone());
        }
        if !matches!(
            checkout.state,
            WorkflowState::Published
                | WorkflowState::Rejected
                | WorkflowState::Cancelled
                | WorkflowState::Failed
        ) {
            return Err(WorkError::Conflict(
                "only a terminal checkout can be cleaned up".into(),
            ));
        }

        let mut next = current.clone();
        let submission_id = next
            .submissions
            .values()
            .filter(|submission| submission.checkout_id == checkout_id)
            .max_by_key(|submission| submission.created_at)
            .map(|submission| submission.submission_id.clone());
        if let Some(submission_id) = submission_id {
            transition_records(
                &mut next,
                &submission_id,
                WorkflowState::Closed,
                ReceiptKind::Cleanup,
                &request.actor,
                "temporary checkout and integration data removed",
                Some("cleanup_complete".into()),
            )?;
        } else {
            close_record(&mut next, checkout_id, &request.actor)?;
        }
        next.checkouts
            .get_mut(checkout_id)
            .expect("checkout remains present")
            .lease = None;
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: checkout_id.to_string(),
            },
        );
        let result = next
            .checkouts
            .get(checkout_id)
            .cloned()
            .expect("checkout remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }
}
