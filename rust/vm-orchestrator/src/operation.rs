use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Operation {
    pub id: String,
    pub workspace_id: String,
    pub operation_type: OperationType,
    pub status: OperationStatus,
    #[schema(value_type = String, format = Date)]
    pub started_at: DateTime<Utc>,
    #[schema(value_type = Option<String>, format = Date)]
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(rename_all = "lowercase")]
pub enum OperationStatus {
    Pending,
    Running,
    Success,
    Failed,
}
