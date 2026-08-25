use chrono::{Duration, Utc};
use vm_packages::{
    sha256_hex, validate_label, CheckoutLease, CheckoutRecord, CreateCheckout, LeaseRecord,
    PackageCheckoutContext, ReceiptKind, SourceKind, WorkflowState, WorkflowTransition,
};

use crate::catalog::source_definition;
use crate::store::{
    ensure_fingerprint, next_id, operation_fingerprint, receipt, IdempotencyRecord, ReceiptInput,
    Store,
};
use crate::{WorkError, WorkResult};

const DEFAULT_LEASE_SECONDS: i64 = 8 * 60 * 60;
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
                package_context: None,
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
            source_only: request.source_only,
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
            package_context: None,
        })
    }

    pub async fn record_source(
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

    pub async fn package_checkout_context(
        &self,
        package: &str,
        consumer: &str,
    ) -> WorkResult<PackageCheckoutContext> {
        let database = self.database.lock().await;
        let definition = database
            .packages
            .get(package)
            .ok_or_else(|| WorkError::NotFound(format!("package {package}")))?;
        let pinned_version = database
            .consumers
            .get(consumer)
            .and_then(|record| record.dependencies.get(package))
            .cloned();
        Ok(PackageCheckoutContext {
            ecosystem: definition.ecosystem,
            pinned_version,
        })
    }

    pub async fn matching_active_checkouts(
        &self,
        package: &str,
        consumer: &str,
        workspace_release: bool,
    ) -> WorkResult<Vec<CheckoutRecord>> {
        self.expire_leases().await?;
        Ok(self
            .database
            .lock()
            .await
            .checkouts
            .values()
            .filter(|checkout| {
                checkout.package == package
                    && checkout.workspace_release == workspace_release
                    && checkout.consumers.len() == 1
                    && checkout.consumers[0] == consumer
                    && !checkout.state.revokes_lease()
            })
            .cloned()
            .collect())
    }
}
