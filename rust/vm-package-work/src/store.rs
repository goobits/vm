use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use vm_packages::{
    CheckoutRecord, ConsumerRecord, PackageDefinition, ReleaseRecord, RolloutRecord, SourceKind,
    SubmissionRecord, ToolArtifactRecord, ToolDefinition, ToolPublicationReceipt, WorkflowReceipt,
};

use crate::{io::atomic_write, WorkResult};

mod idempotency;
mod receipt;
pub(crate) use idempotency::{
    ensure_fingerprint, next_id, operation_fingerprint, validate_key as validate_idempotency_key,
};
pub(crate) use receipt::{receipt, ReceiptInput};

const STATE_FILE: &str = "state/workflows.json";

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

#[derive(Debug, Clone)]
pub(crate) struct SourceDefinition {
    pub(crate) kind: SourceKind,
    pub(crate) name: String,
    pub(crate) repository: String,
    pub(crate) default_branch: String,
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

    pub(crate) async fn commit(
        &self,
        current: &mut tokio::sync::MutexGuard<'_, Database>,
        next: Database,
    ) -> WorkResult<()> {
        persist_database(&self.root, &next).await?;
        **current = next;
        self.materialize_receipts_locked(current).await
    }
}

async fn persist_database(root: &Path, database: &Database) -> WorkResult<()> {
    atomic_write(root.join(STATE_FILE), pretty_json(database)?).await
}

pub(crate) fn pretty_json(value: &impl Serialize) -> WorkResult<Vec<u8>> {
    let mut content = serde_json::to_vec_pretty(value)?;
    content.push(b'\n');
    Ok(content)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
