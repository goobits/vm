use chrono::Utc;
use vm_packages::{
    sha256_hex, validate_label, CreateCheckout, LeaseRecord, LeaseRequest, ReceiptKind,
    TransitionRequest, WorkflowState, WorkflowTransition,
};

use crate::store::{receipt, Database, ReceiptInput, MAX_LEASE_SECONDS};
use crate::{WorkError, WorkResult};

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
    Ok(())
}

pub(crate) fn id(package: &str, now: chrono::DateTime<Utc>, sequence: u64) -> String {
    let slug = package
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    format!("pkg-{slug}-{}-{sequence:06}", now.format("%Y%m%d"))
}

pub(crate) fn normalized_consumers(mut consumers: Vec<String>) -> Vec<String> {
    consumers.sort();
    consumers.dedup();
    consumers
}

pub(crate) fn validate_create(request: &CreateCheckout) -> WorkResult<()> {
    validate_label("package", &request.package)?;
    validate_label("agent", &request.agent)?;
    for consumer in &request.consumers {
        validate_label("consumer", consumer)?;
    }
    if request.task.trim().is_empty() || request.task.len() > 1_000 {
        return Err(WorkError::Invalid(
            "task must contain 1 to 1000 characters".into(),
        ));
    }
    if !(32..=256).contains(&request.lease_token.len()) {
        return Err(WorkError::Invalid(
            "lease token must contain 32 to 256 characters".into(),
        ));
    }
    crate::store::validate_idempotency_key(&request.idempotency_key)
}

pub(crate) fn validate_lease_request(request: &LeaseRequest) -> WorkResult<()> {
    validate_label("lease holder", &request.holder)?;
    if request.lease_token.trim().is_empty() {
        return Err(WorkError::Invalid("lease token cannot be empty".into()));
    }
    if !(60..=MAX_LEASE_SECONDS).contains(&request.duration_seconds) {
        return Err(WorkError::Invalid(format!(
            "lease duration must be between 60 and {MAX_LEASE_SECONDS} seconds"
        )));
    }
    crate::store::validate_idempotency_key(&request.idempotency_key)
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

pub(crate) fn validate_lease(
    lease: &LeaseRecord,
    holder: &str,
    token: &str,
    now: chrono::DateTime<Utc>,
) -> WorkResult<()> {
    if lease.expires_at <= now {
        return Err(WorkError::Conflict("checkout lease has expired".into()));
    }
    if lease.holder != holder || lease.token_digest != sha256_hex(token) {
        return Err(WorkError::Unauthorized(
            "checkout lease holder or token did not match".into(),
        ));
    }
    Ok(())
}
