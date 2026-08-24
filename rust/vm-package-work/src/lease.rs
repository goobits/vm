use chrono::{Duration, Utc};
use vm_packages::{
    sha256_hex, validate_label, CheckoutRecord, LeaseRecord, LeaseRequest, ReceiptKind,
};

use crate::store::{
    ensure_fingerprint, operation_fingerprint, receipt, IdempotencyRecord, ReceiptInput,
};
use crate::{Store, WorkError, WorkResult};

const MAX_LEASE_SECONDS: i64 = 24 * 60 * 60;

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

impl Store {
    pub async fn authorize_lease(
        &self,
        checkout_id: &str,
        consumer: &str,
        token: &str,
    ) -> WorkResult<CheckoutRecord> {
        validate_label("consumer", consumer)?;
        let checkout = self.get_checkout(checkout_id).await?;
        if !checkout
            .consumers
            .iter()
            .any(|candidate| candidate == consumer)
        {
            return Err(WorkError::Unauthorized(
                "checkout is not assigned to this consumer".into(),
            ));
        }
        let lease = checkout
            .lease
            .as_ref()
            .ok_or_else(|| WorkError::Conflict("checkout has no active lease".into()))?;
        if lease.expires_at <= Utc::now() || lease.token_digest != sha256_hex(token) {
            return Err(WorkError::Unauthorized(
                "invalid or expired checkout lease".into(),
            ));
        }
        Ok(checkout)
    }
    pub async fn renew_lease(
        &self,
        checkout_id: &str,
        request: LeaseRequest,
    ) -> WorkResult<CheckoutRecord> {
        self.update_lease(checkout_id, request, false).await
    }

    /// Replace a checkout lease after authenticating the checkout's assigned
    /// consumer at the server boundary. Unlike ordinary renewal, this permits
    /// a restarted guest to rotate a lost lease credential.
    pub async fn reacquire_lease(
        &self,
        checkout_id: &str,
        request: LeaseRequest,
    ) -> WorkResult<CheckoutRecord> {
        self.update_lease(checkout_id, request, true).await
    }

    async fn update_lease(
        &self,
        checkout_id: &str,
        request: LeaseRequest,
        trusted_reacquire: bool,
    ) -> WorkResult<CheckoutRecord> {
        validate_lease_request(&request)?;
        let operation = if trusted_reacquire {
            "reacquire_lease"
        } else {
            "renew_lease"
        };
        let fingerprint = operation_fingerprint(operation, Some(checkout_id), &request)?;
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
        let now = Utc::now();
        let checkout = next
            .checkouts
            .get_mut(checkout_id)
            .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?;
        let reacquired = if trusted_reacquire {
            if checkout.state.revokes_lease() {
                return Err(WorkError::Conflict(
                    "terminal checkout cannot reacquire a lease".into(),
                ));
            }
            if checkout.agent != request.holder {
                return Err(WorkError::Unauthorized(
                    "checkout lease holder did not match".into(),
                ));
            }
            checkout.lease = Some(LeaseRecord {
                holder: request.holder.clone(),
                token_digest: sha256_hex(&request.lease_token),
                expires_at: now + Duration::seconds(request.duration_seconds),
            });
            next.lease_credentials
                .insert(checkout_id.to_string(), sha256_hex(&request.lease_token));
            true
        } else if let Some(lease) = checkout.lease.as_mut() {
            validate_lease(lease, &request.holder, &request.lease_token, now)?;
            lease.expires_at = now + Duration::seconds(request.duration_seconds);
            false
        } else {
            if checkout.state.revokes_lease() {
                return Err(WorkError::Conflict(
                    "terminal checkout cannot reacquire a lease".into(),
                ));
            }
            if checkout.agent != request.holder {
                return Err(WorkError::Unauthorized(
                    "checkout lease holder did not match".into(),
                ));
            }
            let credential = next.lease_credentials.get(checkout_id).ok_or_else(|| {
                WorkError::Conflict("checkout lease credential is unavailable".into())
            })?;
            if credential != &sha256_hex(&request.lease_token) {
                return Err(WorkError::Unauthorized(
                    "checkout lease token did not match".into(),
                ));
            }
            checkout.lease = Some(LeaseRecord {
                holder: request.holder.clone(),
                token_digest: sha256_hex(&request.lease_token),
                expires_at: now + Duration::seconds(request.duration_seconds),
            });
            true
        };
        checkout.updated_at = now;
        let state = checkout.state;
        let result = checkout.clone();
        let receipt = receipt(
            &mut next,
            ReceiptInput {
                kind: if reacquired {
                    ReceiptKind::LeaseAcquired
                } else {
                    ReceiptKind::LeaseRenewed
                },
                checkout_id,
                actor: &request.holder,
                previous: Some(state),
                next: state,
                commit: None,
                validation_result: None,
                reason: if reacquired {
                    "expired checkout lease reacquired"
                } else {
                    "checkout lease renewed"
                },
                timestamp: now,
            },
        );
        next.receipts.insert(receipt.receipt_id.clone(), receipt);
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: checkout_id.to_string(),
            },
        );
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn release_lease(
        &self,
        checkout_id: &str,
        request: LeaseRequest,
    ) -> WorkResult<CheckoutRecord> {
        validate_lease_request(&request)?;
        let fingerprint = operation_fingerprint("release_lease", Some(checkout_id), &request)?;
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
        let now = Utc::now();
        let checkout = next
            .checkouts
            .get_mut(checkout_id)
            .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?;
        let lease = checkout
            .lease
            .as_ref()
            .ok_or_else(|| WorkError::Conflict("checkout has no active lease".into()))?;
        validate_lease(lease, &request.holder, &request.lease_token, now)?;
        checkout.lease = None;
        next.lease_credentials.remove(checkout_id);
        checkout.updated_at = now;
        let state = checkout.state;
        let result = checkout.clone();
        let receipt = receipt(
            &mut next,
            ReceiptInput {
                kind: ReceiptKind::LeaseReleased,
                checkout_id,
                actor: &request.holder,
                previous: Some(state),
                next: state,
                commit: None,
                validation_result: None,
                reason: "checkout lease released",
                timestamp: now,
            },
        );
        next.receipts.insert(receipt.receipt_id.clone(), receipt);
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: checkout_id.to_string(),
            },
        );
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub(crate) async fn expire_leases(&self) -> WorkResult<()> {
        let mut current = self.database.lock().await;
        let now = Utc::now();
        if !current.checkouts.values().any(|checkout| {
            checkout
                .lease
                .as_ref()
                .is_some_and(|lease| lease.expires_at <= now)
        }) {
            return Ok(());
        }
        let mut next = current.clone();
        let expired = next
            .checkouts
            .iter()
            .filter_map(|(id, checkout)| {
                checkout
                    .lease
                    .as_ref()
                    .filter(|lease| lease.expires_at <= now)
                    .map(|lease| (id.clone(), lease.holder.clone(), checkout.state))
            })
            .collect::<Vec<_>>();
        for (checkout_id, holder, state) in expired {
            if let Some(checkout) = next.checkouts.get_mut(&checkout_id) {
                checkout.lease = None;
                checkout.updated_at = now;
            }
            let receipt = receipt(
                &mut next,
                ReceiptInput {
                    kind: ReceiptKind::LeaseReleased,
                    checkout_id: &checkout_id,
                    actor: &holder,
                    previous: Some(state),
                    next: state,
                    commit: None,
                    validation_result: None,
                    reason: "expired lease recovered",
                    timestamp: now,
                },
            );
            next.receipts.insert(receipt.receipt_id.clone(), receipt);
        }
        self.commit(&mut current, next).await
    }
}
