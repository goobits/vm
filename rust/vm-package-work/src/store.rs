use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use vm_packages::{
    CheckoutLease, CheckoutRecord, CreateCheckout, LeaseRecord, LeaseRequest, ReceiptKind,
    TransitionRequest, WorkflowReceipt, WorkflowState, WorkflowTransition,
};

use crate::{WorkError, WorkResult};

const STATE_FILE: &str = "state/workflows.json";
const DEFAULT_LEASE_SECONDS: i64 = 30 * 60;
const MAX_LEASE_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdempotencyRecord {
    fingerprint: String,
    checkout_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Database {
    #[serde(default)]
    sequence: u64,
    #[serde(default)]
    checkouts: BTreeMap<String, CheckoutRecord>,
    #[serde(default)]
    receipts: BTreeMap<String, WorkflowReceipt>,
    #[serde(default)]
    idempotency: BTreeMap<String, IdempotencyRecord>,
}

pub struct Store {
    root: PathBuf,
    database: Mutex<Database>,
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

    pub async fn create_checkout(&self, request: CreateCheckout) -> WorkResult<CheckoutLease> {
        validate_create(&request)?;
        let fingerprint = operation_fingerprint("create", None, &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            let checkout = current
                .checkouts
                .get(&existing.checkout_id)
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
                checkout_id: checkout_id.clone(),
            },
        );
        next.checkouts.insert(checkout_id, record.clone());
        self.commit(&mut current, next).await?;
        Ok(CheckoutLease {
            checkout: record,
            lease_token: Some(token),
        })
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
                .get(&existing.checkout_id)
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
                checkout_id: checkout_id.to_string(),
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
                .get(&existing.checkout_id)
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
                checkout_id: checkout_id.to_string(),
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
                .get(&existing.checkout_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let mut next = current.clone();
        let now = Utc::now();
        let checkout = next
            .checkouts
            .get_mut(checkout_id)
            .ok_or_else(|| WorkError::NotFound(checkout_id.to_string()))?;
        let previous = checkout.state;
        if !previous.can_transition_to(request.next) {
            return Err(WorkError::Conflict(format!(
                "cannot transition from {previous:?} to {:?}",
                request.next
            )));
        }
        checkout.state = request.next;
        checkout.updated_at = now;
        let receipt = receipt(
            &mut next,
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
        let checkout = next
            .checkouts
            .get_mut(checkout_id)
            .expect("checkout remains present");
        checkout.transitions.push(WorkflowTransition {
            timestamp: now,
            actor: request.actor,
            previous: Some(previous),
            next: request.next,
            commit: request.commit,
            validation_result: request.validation_result,
            reason: request.reason,
            receipt_id: receipt.receipt_id.clone(),
        });
        let result = checkout.clone();
        next.receipts.insert(receipt.receipt_id.clone(), receipt);
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                checkout_id: checkout_id.to_string(),
            },
        );
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

    async fn commit(
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
        Ok(())
    }
}

struct ReceiptInput<'a> {
    kind: ReceiptKind,
    checkout_id: &'a str,
    actor: &'a str,
    previous: Option<WorkflowState>,
    next: WorkflowState,
    commit: Option<String>,
    validation_result: Option<String>,
    reason: &'a str,
    timestamp: chrono::DateTime<Utc>,
}

fn receipt(database: &mut Database, input: ReceiptInput<'_>) -> WorkflowReceipt {
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

fn next_id(database: &mut Database) -> u64 {
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

fn validate_label(field: &str, value: &str) -> WorkResult<()> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && !value.contains("..")
        && !value.starts_with('/')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '/' | '@')
        });
    if valid {
        Ok(())
    } else {
        Err(WorkError::Invalid(format!("invalid {field}")))
    }
}

fn validate_idempotency_key(key: &str) -> WorkResult<()> {
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

fn ensure_fingerprint(existing: &IdempotencyRecord, fingerprint: &str) -> WorkResult<()> {
    if existing.fingerprint == fingerprint {
        Ok(())
    } else {
        Err(WorkError::Conflict(
            "idempotency key was already used for a different request".into(),
        ))
    }
}

fn operation_fingerprint(
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

async fn atomic_write(path: PathBuf, content: Vec<u8>) -> WorkResult<()> {
    tokio::task::spawn_blocking(move || vm_core::file_system::atomic_write(&path, &content))
        .await
        .map_err(|error| WorkError::Internal(format!("atomic write task failed: {error}")))??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
