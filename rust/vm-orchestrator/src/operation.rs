use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    pub workspace_id: String,
    pub operation_type: OperationType,
    pub status: OperationStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
pub enum OperationType {
    Create,
    Delete,
    Start,
    Stop,
    Restart,
    Rebuild,
    Snapshot,
    SnapshotRestore,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
pub enum OperationStatus {
    Pending,
    Running,
    Success,
    Failed,
}
