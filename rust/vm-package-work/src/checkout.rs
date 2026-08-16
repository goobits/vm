use chrono::{Duration, Utc};
use vm_packages::{
    sha256_hex, validate_label, CheckoutLease, CheckoutRecord, CleanupRequest, CreateCheckout,
    LeaseRecord, LeaseRequest, ReceiptKind, SourceKind, TransitionRequest, WorkflowState,
    WorkflowTransition,
};

use crate::catalog::source_definition;
use crate::store::{
    ensure_fingerprint, next_id, operation_fingerprint, receipt, validate_idempotency_key,
    Database, IdempotencyRecord, ReceiptInput, Store,
};
use crate::submission::transition_records;
use crate::{WorkError, WorkResult};

const DEFAULT_LEASE_SECONDS: i64 = 8 * 60 * 60;
const MAX_LEASE_SECONDS: i64 = 24 * 60 * 60;

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

impl Store {
    pub async fn create_checkout(&self, request: CreateCheckout) -> WorkResult<CheckoutLease> {
        validate_create(&request)?;
        let fingerprint = operation_fingerprint("create", None, &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            let checkout = current
                .checkouts
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()))?;
            return Ok(CheckoutLease {
                checkout,
                lease_token: Some(request.lease_token),
            });
        }

        let source = source_definition(&current, &request.package)?;
        let source_kind = if request.workspace_release {
            let source = source.ok_or_else(|| {
                WorkError::NotFound(format!("registered source {}", request.package))
            })?;
            if !source.workspace_release {
                return Err(WorkError::Unauthorized(
                    "workspace release requires a source registered from a configured root".into(),
                ));
            }
            source.kind
        } else {
            source.map_or(SourceKind::Package, |source| source.kind)
        };
        let mut next = current.clone();
        let now = Utc::now();
        let checkout_id = id(&request.package, now, next_id(&mut next));
        let lease = LeaseRecord {
            holder: request.agent.clone(),
            token_digest: sha256_hex(&request.lease_token),
            expires_at: now + Duration::seconds(DEFAULT_LEASE_SECONDS),
        };
        let checkout_receipt = receipt(
            &mut next,
            ReceiptInput {
                kind: ReceiptKind::Checkout,
                checkout_id: &checkout_id,
                actor: &request.agent,
                previous: None,
                next: WorkflowState::Created,
                commit: None,
                validation_result: None,
                reason: &request.task,
                timestamp: now,
            },
        );
        let lease_receipt = receipt(
            &mut next,
            ReceiptInput {
                kind: ReceiptKind::LeaseAcquired,
                checkout_id: &checkout_id,
                actor: &request.agent,
                previous: Some(WorkflowState::Created),
                next: WorkflowState::Created,
                commit: None,
                validation_result: None,
                reason: "initial checkout lease acquired",
                timestamp: now,
            },
        );
        let record = CheckoutRecord {
            checkout_id: checkout_id.clone(),
            package: request.package.clone(),
            source_kind,
            agent: request.agent.clone(),
            consumers: normalized_consumers(request.consumers),
            task: request.task,
            workspace_release: request.workspace_release,
            initial_release: false,
            state: WorkflowState::Created,
            base_branch: None,
            base_commit: None,
            branch: None,
            worktree: None,
            lease: Some(lease),
            created_at: now,
            updated_at: now,
            transitions: vec![WorkflowTransition {
                timestamp: now,
                actor: request.agent,
                previous: None,
                next: WorkflowState::Created,
                commit: None,
                validation_result: None,
                reason: "checkout created".into(),
                receipt_id: checkout_receipt.receipt_id.clone(),
            }],
        };
        next.receipts
            .insert(checkout_receipt.receipt_id.clone(), checkout_receipt);
        next.receipts
            .insert(lease_receipt.receipt_id.clone(), lease_receipt);
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: checkout_id.clone(),
            },
        );
        next.lease_credentials
            .insert(checkout_id.clone(), sha256_hex(&request.lease_token));
        next.checkouts.insert(checkout_id, record.clone());
        self.commit(&mut current, next).await?;
        Ok(CheckoutLease {
            checkout: record,
            lease_token: Some(request.lease_token),
        })
    }

    pub async fn record_source(
        &self,
        checkout_id: &str,
        base_branch: String,
        base_commit: String,
        branch: String,
        worktree: String,
    ) -> WorkResult<CheckoutRecord> {
        self.record_source_with_baseline(
            checkout_id,
            base_branch,
            base_commit,
            branch,
            worktree,
            false,
        )
        .await
    }

    pub async fn record_workspace_source(
        &self,
        checkout_id: &str,
        base_branch: String,
        base_commit: String,
        branch: String,
        worktree: String,
        initial_release: bool,
    ) -> WorkResult<CheckoutRecord> {
        self.record_source_with_baseline(
            checkout_id,
            base_branch,
            base_commit,
            branch,
            worktree,
            initial_release,
        )
        .await
    }

    async fn record_source_with_baseline(
        &self,
        checkout_id: &str,
        base_branch: String,
        base_commit: String,
        branch: String,
        worktree: String,
        initial_release: bool,
    ) -> WorkResult<CheckoutRecord> {
        let mut current = self.database.lock().await;
        let mut next = current.clone();
        let now = Utc::now();
        let checkout = next
            .checkouts
            .get_mut(checkout_id)
            .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?;
        if checkout.state != WorkflowState::Created {
            return Err(WorkError::Conflict(
                "source can only be attached to a created checkout".into(),
            ));
        }
        checkout.base_branch = Some(base_branch);
        checkout.base_commit = Some(base_commit.clone());
        checkout.initial_release = initial_release;
        checkout.branch = Some(branch);
        checkout.worktree = Some(worktree);
        checkout.state = WorkflowState::CheckedOut;
        checkout.updated_at = now;
        let actor = checkout.agent.clone();
        let receipt = receipt(
            &mut next,
            ReceiptInput {
                kind: ReceiptKind::SourcePrepared,
                checkout_id,
                actor: &actor,
                previous: Some(WorkflowState::Created),
                next: WorkflowState::CheckedOut,
                commit: Some(base_commit),
                validation_result: None,
                reason: "isolated source checkout prepared",
                timestamp: now,
            },
        );
        let checkout = next
            .checkouts
            .get_mut(checkout_id)
            .expect("checkout remains present");
        checkout.transitions.push(WorkflowTransition {
            timestamp: now,
            actor,
            previous: Some(WorkflowState::Created),
            next: WorkflowState::CheckedOut,
            commit: receipt.commit.clone(),
            validation_result: None,
            reason: receipt.reason.clone(),
            receipt_id: receipt.receipt_id.clone(),
        });
        let result = checkout.clone();
        next.receipts.insert(receipt.receipt_id.clone(), receipt);
        self.commit(&mut current, next).await?;
        Ok(result)
    }

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

    pub async fn get_checkout(&self, checkout_id: &str) -> WorkResult<CheckoutRecord> {
        self.expire_leases().await?;
        self.database
            .lock()
            .await
            .checkouts
            .get(checkout_id)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))
    }

    pub async fn list_checkouts(&self) -> WorkResult<Vec<CheckoutRecord>> {
        self.expire_leases().await?;
        Ok(self
            .database
            .lock()
            .await
            .checkouts
            .values()
            .cloned()
            .collect())
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
