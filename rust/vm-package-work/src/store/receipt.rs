use chrono::Utc;
use vm_core::file_system::atomic_write_async;
use vm_packages::{ReceiptKind, WorkflowReceipt, WorkflowState};

use super::idempotency::next_id;
use super::{pretty_json, Database, Store};
use crate::{WorkError, WorkResult};

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

impl Store {
    pub async fn get_receipt(&self, receipt_id: &str) -> WorkResult<WorkflowReceipt> {
        self.database
            .lock()
            .await
            .receipts
            .get(receipt_id)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(receipt_id.to_string()))
    }

    pub(crate) async fn materialize_receipts(&self) -> WorkResult<()> {
        let database = self.database.lock().await;
        self.materialize_receipts_locked(&database).await
    }

    pub(crate) async fn materialize_receipts_locked(&self, database: &Database) -> WorkResult<()> {
        for receipt in database.receipts.values() {
            let path = self
                .root()
                .join("receipts")
                .join(format!("{}.json", receipt.receipt_id));
            atomic_write_async(path, pretty_json(receipt)?).await?;
        }
        let releases = self.root().join("receipts/releases");
        tokio::fs::create_dir_all(&releases).await?;
        for release in database.releases.values() {
            atomic_write_async(
                releases.join(format!("{}.json", release.release_id)),
                pretty_json(release)?,
            )
            .await?;
        }
        let consumers = self.root().join("receipts/consumers");
        tokio::fs::create_dir_all(&consumers).await?;
        for consumer in database.consumers.values() {
            atomic_write_async(
                consumers.join(format!("{}.json", consumer.name)),
                pretty_json(consumer)?,
            )
            .await?;
        }
        let rollouts = self.root().join("receipts/rollouts");
        tokio::fs::create_dir_all(&rollouts).await?;
        for rollout in database.rollouts.values() {
            atomic_write_async(
                rollouts.join(format!("{}.json", rollout.rollout_id)),
                pretty_json(rollout)?,
            )
            .await?;
        }
        let tools = self.root().join("receipts/tools");
        tokio::fs::create_dir_all(&tools).await?;
        for receipt in database.tool_receipts.values() {
            atomic_write_async(
                tools.join(format!("{}.json", receipt.receipt_id)),
                pretty_json(receipt)?,
            )
            .await?;
        }
        Ok(())
    }
}
