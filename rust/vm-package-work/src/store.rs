use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use vm_packages::{
    sha256_hex, validate_label, CheckoutLease, CheckoutRecord, CleanupRequest, ConsumerRecord,
    CreateCheckout, InternalPackageCatalog, LeaseRecord, LeaseRequest, PackageDefinition,
    ReceiptKind, RegisterPackage, ReleaseRecord, RolloutRecord, SubmissionRecord,
    ToolArtifactRecord, ToolDefinition, ToolPublicationReceipt, TransitionRequest, WorkflowReceipt,
    WorkflowState, WorkflowTransition,
};

use crate::submission::transition_records;
use crate::{io::atomic_write, WorkError, WorkResult};

mod support;
use support::{
    checkout_id, close_checkout_record, normalized_consumers, persist_database, pretty_json,
    transition_checkout_record, validate_create, validate_lease, validate_lease_request,
    validate_transition,
};
pub(crate) use support::{
    ensure_fingerprint, next_id, operation_fingerprint, receipt, validate_idempotency_key,
    ReceiptInput,
};

const STATE_FILE: &str = "state/workflows.json";
const CATALOG_FILE: &str = "catalog/packages.json";
const DEFAULT_LEASE_SECONDS: i64 = 8 * 60 * 60;
const MAX_LEASE_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IdempotencyRecord {
    pub(crate) fingerprint: String,
    #[serde(alias = "checkout_id")]
    pub(crate) target_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Database {
    #[serde(default)]
    pub(crate) sequence: u64,
    #[serde(default)]
    pub(crate) checkouts: BTreeMap<String, CheckoutRecord>,
    #[serde(default)]
    pub(crate) receipts: BTreeMap<String, WorkflowReceipt>,
    #[serde(default)]
    pub(crate) idempotency: BTreeMap<String, IdempotencyRecord>,
    #[serde(default)]
    pub(crate) packages: BTreeMap<String, PackageDefinition>,
    #[serde(default)]
    pub(crate) submissions: BTreeMap<String, SubmissionRecord>,
    #[serde(default)]
    pub(crate) releases: BTreeMap<String, ReleaseRecord>,
    #[serde(default)]
    pub(crate) consumers: BTreeMap<String, ConsumerRecord>,
    #[serde(default)]
    pub(crate) rollouts: BTreeMap<String, RolloutRecord>,
    #[serde(default)]
    pub(crate) tools: BTreeMap<String, ToolDefinition>,
    #[serde(default)]
    pub(crate) tool_artifacts: BTreeMap<String, ToolArtifactRecord>,
    #[serde(default)]
    pub(crate) tool_receipts: BTreeMap<String, ToolPublicationReceipt>,
}

pub(crate) struct Store {
    root: PathBuf,
    pub(crate) database: Mutex<Database>,
}

impl Store {
    pub async fn open(root: impl Into<PathBuf>) -> WorkResult<Self> {
        let root = root.into();
        tokio::fs::create_dir_all(root.join("state")).await?;
        tokio::fs::create_dir_all(root.join("receipts")).await?;
        tokio::fs::create_dir_all(root.join("catalog")).await?;
        let path = root.join(STATE_FILE);
        let database = match tokio::fs::read(&path).await {
            Ok(content) => serde_json::from_slice(&content)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Database::default(),
            Err(error) => return Err(error.into()),
        };
        let store = Self {
            root,
            database: Mutex::new(database),
        };
        store.expire_leases().await?;
        store.materialize_receipts().await?;
        store.materialize_catalog().await?;
        Ok(store)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

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

        let mut next = current.clone();
        let now = Utc::now();
        let checkout_id = checkout_id(&request.package, now, next_id(&mut next));
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
            agent: request.agent.clone(),
            consumers: normalized_consumers(request.consumers),
            task: request.task,
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
        next.checkouts.insert(checkout_id, record.clone());
        self.commit(&mut current, next).await?;
        Ok(CheckoutLease {
            checkout: record,
            lease_token: Some(request.lease_token),
        })
    }

    pub async fn register_package(
        &self,
        request: RegisterPackage,
    ) -> WorkResult<PackageDefinition> {
        request.validate()?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.packages.get(&request.name).cloned() {
            if existing.ecosystem == request.ecosystem
                && existing.repository == request.repository
                && existing.default_branch == request.default_branch
            {
                self.materialize_catalog_locked(&current).await?;
                return Ok(existing);
            }
            return Err(WorkError::Conflict(format!(
                "package '{}' is already registered with different settings",
                request.name
            )));
        }
        let definition = PackageDefinition {
            name: request.name,
            ecosystem: request.ecosystem,
            repository: request.repository,
            default_branch: request.default_branch,
            registered_at: Utc::now(),
        };
        let mut next = current.clone();
        next.packages
            .insert(definition.name.clone(), definition.clone());
        self.commit(&mut current, next).await?;
        self.materialize_catalog_locked(&current).await?;
        Ok(definition)
    }

    pub async fn package(&self, name: &str) -> WorkResult<PackageDefinition> {
        self.database
            .lock()
            .await
            .packages
            .get(name)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("package {name}")))
    }

    pub async fn packages(&self) -> Vec<PackageDefinition> {
        self.database
            .lock()
            .await
            .packages
            .values()
            .cloned()
            .collect()
    }

    pub async fn internal_catalog(&self) -> WorkResult<InternalPackageCatalog> {
        let database = self.database.lock().await;
        InternalPackageCatalog::from_definitions(database.packages.values()).map_err(Into::into)
    }

    pub async fn record_source(
        &self,
        checkout_id: &str,
        base_branch: String,
        base_commit: String,
        branch: String,
        worktree: String,
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
        validate_lease_request(&request)?;
        let fingerprint = operation_fingerprint("renew_lease", Some(checkout_id), &request)?;
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
            .as_mut()
            .ok_or_else(|| WorkError::Conflict("checkout has no active lease".into()))?;
        validate_lease(lease, &request.holder, &request.lease_token, now)?;
        lease.expires_at = now + Duration::seconds(request.duration_seconds);
        checkout.updated_at = now;
        let state = checkout.state;
        let result = checkout.clone();
        let receipt = receipt(
            &mut next,
            ReceiptInput {
                kind: ReceiptKind::LeaseRenewed,
                checkout_id,
                actor: &request.holder,
                previous: Some(state),
                next: state,
                commit: None,
                validation_result: None,
                reason: "checkout lease renewed",
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
            transition_checkout_record(&mut next, checkout_id, &request)?;
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
            close_checkout_record(&mut next, checkout_id, &request.actor)?;
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

    pub async fn get_receipt(&self, receipt_id: &str) -> WorkResult<WorkflowReceipt> {
        self.database
            .lock()
            .await
            .receipts
            .get(receipt_id)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(receipt_id.to_string()))
    }

    async fn expire_leases(&self) -> WorkResult<()> {
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

    pub(crate) async fn commit(
        &self,
        current: &mut tokio::sync::MutexGuard<'_, Database>,
        next: Database,
    ) -> WorkResult<()> {
        persist_database(&self.root, &next).await?;
        **current = next;
        self.materialize_receipts_locked(current).await
    }

    async fn materialize_receipts(&self) -> WorkResult<()> {
        let database = self.database.lock().await;
        self.materialize_receipts_locked(&database).await
    }

    async fn materialize_catalog(&self) -> WorkResult<()> {
        let database = self.database.lock().await;
        self.materialize_catalog_locked(&database).await
    }

    async fn materialize_catalog_locked(&self, database: &Database) -> WorkResult<()> {
        let catalog = InternalPackageCatalog::from_definitions(database.packages.values())?;
        atomic_write(self.root.join(CATALOG_FILE), pretty_json(&catalog)?).await
    }

    async fn materialize_receipts_locked(&self, database: &Database) -> WorkResult<()> {
        for receipt in database.receipts.values() {
            let path = self
                .root
                .join("receipts")
                .join(format!("{}.json", receipt.receipt_id));
            let content = pretty_json(receipt)?;
            atomic_write(path, content).await?;
        }
        let releases = self.root.join("receipts/releases");
        tokio::fs::create_dir_all(&releases).await?;
        for release in database.releases.values() {
            atomic_write(
                releases.join(format!("{}.json", release.release_id)),
                pretty_json(release)?,
            )
            .await?;
        }
        let consumers = self.root.join("receipts/consumers");
        tokio::fs::create_dir_all(&consumers).await?;
        for consumer in database.consumers.values() {
            atomic_write(
                consumers.join(format!("{}.json", consumer.name)),
                pretty_json(consumer)?,
            )
            .await?;
        }
        let rollouts = self.root.join("receipts/rollouts");
        tokio::fs::create_dir_all(&rollouts).await?;
        for rollout in database.rollouts.values() {
            atomic_write(
                rollouts.join(format!("{}.json", rollout.rollout_id)),
                pretty_json(rollout)?,
            )
            .await?;
        }
        let tools = self.root.join("receipts/tools");
        tokio::fs::create_dir_all(&tools).await?;
        for receipt in database.tool_receipts.values() {
            atomic_write(
                tools.join(format!("{}.json", receipt.receipt_id)),
                pretty_json(receipt)?,
            )
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
