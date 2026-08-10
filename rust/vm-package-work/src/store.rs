use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use vm_packages::{
    validate_label, CheckoutLease, CheckoutRecord, CleanupRequest, ConsumerRecord, CreateCheckout,
    LeaseRecord, LeaseRequest, PackageDefinition, ReceiptKind, RegisterPackage, ReleaseRecord,
    RolloutRecord, SubmissionRecord, TransitionRequest, WorkflowReceipt, WorkflowState,
    WorkflowTransition,
};

use crate::submission::transition_records;
use crate::{io::atomic_write, WorkError, WorkResult};

const STATE_FILE: &str = "state/workflows.json";
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
}

pub struct Store {
    root: PathBuf,
    pub(crate) database: Mutex<Database>,
}

impl Store {
    pub async fn open(root: impl Into<PathBuf>) -> WorkResult<Self> {
        let root = root.into();
        tokio::fs::create_dir_all(root.join("state")).await?;
        tokio::fs::create_dir_all(root.join("receipts")).await?;
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
                lease_token: None,
            });
        }

        let mut next = current.clone();
        let now = Utc::now();
        let checkout_id = checkout_id(&request.package, now, next_id(&mut next));
        let token = vm_core::secrets::generate_random_password(48);
        let lease = LeaseRecord {
            holder: request.agent.clone(),
            token_digest: digest(&token),
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
            lease_token: Some(token),
        })
    }

    pub async fn register_package(
        &self,
        request: RegisterPackage,
    ) -> WorkResult<PackageDefinition> {
        request.validate()?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.packages.get(&request.name) {
            if existing.ecosystem == request.ecosystem
                && existing.repository == request.repository
                && existing.default_branch == request.default_branch
                && existing.ci_registry == request.ci_registry
            {
                return Ok(existing.clone());
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
            ci_registry: request.ci_registry,
            registered_at: Utc::now(),
        };
        let mut next = current.clone();
        next.packages
            .insert(definition.name.clone(), definition.clone());
        self.commit(&mut current, next).await?;
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
        if lease.expires_at <= Utc::now() || lease.token_digest != digest(token) {
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
        Ok(())
    }
}

fn transition_checkout_record(
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

fn close_checkout_record(
    database: &mut Database,
    checkout_id: &str,
    actor: &str,
) -> WorkResult<()> {
    let checkout = database
        .checkouts
        .get(checkout_id)
        .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?;
    let previous = checkout.state;
    let commit = checkout.base_commit.clone();
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

pub(crate) struct ReceiptInput<'a> {
    pub(crate) kind: ReceiptKind,
    pub(crate) checkout_id: &'a str,
    pub(crate) actor: &'a str,
    pub(crate) previous: Option<WorkflowState>,
    pub(crate) next: WorkflowState,
    pub(crate) commit: Option<String>,
    pub(crate) validation_result: Option<String>,
    pub(crate) reason: &'a str,
    pub(crate) timestamp: chrono::DateTime<Utc>,
}

pub(crate) fn receipt(database: &mut Database, input: ReceiptInput<'_>) -> WorkflowReceipt {
    WorkflowReceipt {
        receipt_id: format!("receipt-{:08}", next_id(database)),
        kind: input.kind,
        checkout_id: input.checkout_id.to_string(),
        actor: input.actor.to_string(),
        timestamp: input.timestamp,
        previous: input.previous,
        next: input.next,
        commit: input.commit,
        validation_result: input.validation_result,
        reason: input.reason.to_string(),
    }
}

pub(crate) fn next_id(database: &mut Database) -> u64 {
    database.sequence += 1;
    database.sequence
}

fn checkout_id(package: &str, now: chrono::DateTime<Utc>, sequence: u64) -> String {
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

fn normalized_consumers(mut consumers: Vec<String>) -> Vec<String> {
    consumers.sort();
    consumers.dedup();
    consumers
}

fn validate_create(request: &CreateCheckout) -> WorkResult<()> {
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
    validate_idempotency_key(&request.idempotency_key)
}

fn validate_lease_request(request: &LeaseRequest) -> WorkResult<()> {
    validate_label("lease holder", &request.holder)?;
    if request.lease_token.trim().is_empty() {
        return Err(WorkError::Invalid("lease token cannot be empty".into()));
    }
    if !(60..=MAX_LEASE_SECONDS).contains(&request.duration_seconds) {
        return Err(WorkError::Invalid(format!(
            "lease duration must be between 60 and {MAX_LEASE_SECONDS} seconds"
        )));
    }
    validate_idempotency_key(&request.idempotency_key)
}

fn validate_transition(request: &TransitionRequest) -> WorkResult<()> {
    validate_label("actor", &request.actor)?;
    if request.reason.trim().is_empty() || request.reason.len() > 1_000 {
        return Err(WorkError::Invalid(
            "transition reason must contain 1 to 1000 characters".into(),
        ));
    }
    validate_idempotency_key(&request.idempotency_key)
}

pub(crate) fn validate_idempotency_key(key: &str) -> WorkResult<()> {
    if key.trim().is_empty() || key.len() > 128 {
        Err(WorkError::Invalid(
            "idempotency key must contain 1 to 128 characters".into(),
        ))
    } else {
        Ok(())
    }
}

fn validate_lease(
    lease: &LeaseRecord,
    holder: &str,
    token: &str,
    now: chrono::DateTime<Utc>,
) -> WorkResult<()> {
    if lease.expires_at <= now {
        return Err(WorkError::Conflict("checkout lease has expired".into()));
    }
    if lease.holder != holder || lease.token_digest != digest(token) {
        return Err(WorkError::Unauthorized(
            "checkout lease holder or token did not match".into(),
        ));
    }
    Ok(())
}

pub(crate) fn ensure_fingerprint(
    existing: &IdempotencyRecord,
    fingerprint: &str,
) -> WorkResult<()> {
    if existing.fingerprint == fingerprint {
        Ok(())
    } else {
        Err(WorkError::Conflict(
            "idempotency key was already used for a different request".into(),
        ))
    }
}

pub(crate) fn operation_fingerprint(
    operation: &str,
    checkout_id: Option<&str>,
    value: &impl Serialize,
) -> WorkResult<String> {
    Ok(digest(&serde_json::to_vec(&(
        operation,
        checkout_id,
        value,
    ))?))
}

fn digest(value: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(value.as_ref());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

async fn persist_database(root: &Path, database: &Database) -> WorkResult<()> {
    atomic_write(root.join(STATE_FILE), pretty_json(database)?).await
}

fn pretty_json(value: &impl Serialize) -> WorkResult<Vec<u8>> {
    let mut content = serde_json::to_vec_pretty(value)?;
    content.push(b'\n');
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImportedSubmission;
    use vm_packages::PackageEcosystem;

    fn request(key: &str, agent: &str) -> CreateCheckout {
        CreateCheckout {
            package: "auth".into(),
            agent: agent.into(),
            consumers: vec!["project-b".into(), "project-a".into(), "project-a".into()],
            task: "fix token refresh".into(),
            idempotency_key: key.into(),
        }
    }

    #[tokio::test]
    async fn concurrent_checkouts_are_isolated_and_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();

        let first = store
            .create_checkout(request("one", "agent-1"))
            .await
            .unwrap();
        let retry = store
            .create_checkout(request("one", "agent-1"))
            .await
            .unwrap();
        let second = store
            .create_checkout(request("two", "agent-2"))
            .await
            .unwrap();

        assert_eq!(first.checkout.checkout_id, retry.checkout.checkout_id);
        assert!(retry.lease_token.is_none());
        assert_ne!(first.checkout.checkout_id, second.checkout.checkout_id);
        assert_ne!(first.checkout.lease, second.checkout.lease);
        assert_eq!(first.checkout.consumers, ["project-a", "project-b"]);
    }

    #[tokio::test]
    async fn transitions_are_validated_persisted_and_receipted() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        let checkout = store
            .create_checkout(request("create", "agent-1"))
            .await
            .unwrap();
        let id = &checkout.checkout.checkout_id;

        let checked_out = store
            .transition(
                id,
                TransitionRequest {
                    next: WorkflowState::CheckedOut,
                    actor: "controller".into(),
                    reason: "worktree ready".into(),
                    commit: Some("abc123".into()),
                    validation_result: None,
                    idempotency_key: "transition-1".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(checked_out.state, WorkflowState::CheckedOut);
        assert!(store
            .transition(
                id,
                TransitionRequest {
                    next: WorkflowState::Published,
                    actor: "controller".into(),
                    reason: "skip".into(),
                    commit: None,
                    validation_result: None,
                    idempotency_key: "transition-2".into(),
                },
            )
            .await
            .is_err());

        drop(store);
        let reopened = Store::open(directory.path()).await.unwrap();
        assert_eq!(
            reopened.get_checkout(id).await.unwrap().state,
            WorkflowState::CheckedOut
        );
        assert!(
            directory
                .path()
                .join("receipts")
                .read_dir()
                .unwrap()
                .count()
                >= 3
        );
    }

    #[tokio::test]
    async fn lease_tokens_are_required_and_never_returned_on_retry() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        let created = store
            .create_checkout(request("lease", "agent-1"))
            .await
            .unwrap();
        let id = &created.checkout.checkout_id;
        let token = created.lease_token.unwrap();

        assert!(store
            .renew_lease(
                id,
                LeaseRequest {
                    holder: "agent-1".into(),
                    lease_token: "wrong".into(),
                    duration_seconds: 600,
                    idempotency_key: "bad-renew".into(),
                },
            )
            .await
            .is_err());
        let renewed = store
            .renew_lease(
                id,
                LeaseRequest {
                    holder: "agent-1".into(),
                    lease_token: token,
                    duration_seconds: 600,
                    idempotency_key: "renew".into(),
                },
            )
            .await
            .unwrap();
        assert!(renewed.lease.is_some());
    }

    #[tokio::test]
    async fn catalog_retries_are_exact_and_checkout_archives_are_consumer_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        let package = RegisterPackage {
            name: "auth".into(),
            ecosystem: PackageEcosystem::Cargo,
            repository: "https://example.com/auth.git".into(),
            default_branch: "main".into(),
            ci_registry: None,
        };
        assert_eq!(
            store.register_package(package.clone()).await.unwrap(),
            store.register_package(package).await.unwrap()
        );
        assert!(store
            .register_package(RegisterPackage {
                name: "auth".into(),
                ecosystem: PackageEcosystem::Cargo,
                repository: "https://example.com/other.git".into(),
                default_branch: "main".into(),
                ci_registry: None,
            })
            .await
            .is_err());

        let checkout = store
            .create_checkout(request("scoped", "agent-1"))
            .await
            .unwrap();
        let token = checkout.lease_token.unwrap();
        assert!(store
            .authorize_lease(&checkout.checkout.checkout_id, "project-a", &token)
            .await
            .is_ok());
        assert!(store
            .authorize_lease(&checkout.checkout.checkout_id, "project-c", &token)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn terminal_checkout_cleanup_is_idempotent_without_source() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        let checkout = store
            .create_checkout(request("cleanup", "agent-1"))
            .await
            .unwrap()
            .checkout;
        store
            .transition(
                &checkout.checkout_id,
                TransitionRequest {
                    next: WorkflowState::Failed,
                    actor: "controller".into(),
                    reason: "source preparation failed".into(),
                    commit: None,
                    validation_result: Some("failed".into()),
                    idempotency_key: "fail-cleanup".into(),
                },
            )
            .await
            .unwrap();
        let request = CleanupRequest {
            actor: "controller".into(),
            idempotency_key: "close-cleanup".into(),
        };
        assert_eq!(
            store
                .close_checkout(&checkout.checkout_id, request.clone())
                .await
                .unwrap()
                .state,
            WorkflowState::Closed
        );
        assert_eq!(
            store
                .close_checkout(&checkout.checkout_id, request)
                .await
                .unwrap()
                .state,
            WorkflowState::Closed
        );
    }

    #[tokio::test]
    async fn checkout_transition_keeps_submission_in_sync_when_cancelled() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        let checkout = store
            .create_checkout(request("cancel", "agent-1"))
            .await
            .unwrap()
            .checkout;
        store
            .record_source(
                &checkout.checkout_id,
                "main".into(),
                "a".repeat(40),
                "agents/agent-1/cancel".into(),
                "/data/agents/cancel/source".into(),
            )
            .await
            .unwrap();
        store
            .transition(
                &checkout.checkout_id,
                TransitionRequest {
                    next: WorkflowState::Active,
                    actor: "agent-1".into(),
                    reason: "attached".into(),
                    commit: None,
                    validation_result: None,
                    idempotency_key: "activate-cancel".into(),
                },
            )
            .await
            .unwrap();
        let submission = store
            .record_submission(
                &checkout.checkout_id,
                ImportedSubmission {
                    submitted_commit: "b".repeat(40),
                    diff_digest: "c".repeat(64),
                },
            )
            .await
            .unwrap();
        store
            .transition(
                &checkout.checkout_id,
                TransitionRequest {
                    next: WorkflowState::NeedsChanges,
                    actor: "reviewer".into(),
                    reason: "revise".into(),
                    commit: None,
                    validation_result: Some("needs_changes".into()),
                    idempotency_key: "revise-cancel".into(),
                },
            )
            .await
            .unwrap();
        let cancelled = store
            .transition(
                &checkout.checkout_id,
                TransitionRequest {
                    next: WorkflowState::Cancelled,
                    actor: "controller".into(),
                    reason: "cancelled".into(),
                    commit: None,
                    validation_result: Some("cancelled".into()),
                    idempotency_key: "cancel-checkout".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(cancelled.state, WorkflowState::Cancelled);
        assert!(cancelled.lease.is_none());
        assert_eq!(
            store
                .submission(&submission.submission_id)
                .await
                .unwrap()
                .state,
            WorkflowState::Cancelled
        );
    }
}
