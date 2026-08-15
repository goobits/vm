use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    #[default]
    Package,
    ToolCollection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    Created,
    CheckedOut,
    Active,
    Submitted,
    Validating,
    Reviewing,
    NeedsChanges,
    Approved,
    Integrating,
    ReadyToRelease,
    Publishing,
    Published,
    Rejected,
    Cancelled,
    Failed,
    Closed,
}

impl WorkflowState {
    pub fn can_transition_to(self, next: Self) -> bool {
        use WorkflowState::{
            Active, Approved, Cancelled, CheckedOut, Closed, Created, Failed, Integrating,
            NeedsChanges, Published, Publishing, ReadyToRelease, Rejected, Reviewing, Submitted,
            Validating,
        };
        matches!(
            (self, next),
            (Created, CheckedOut | Cancelled | Failed)
                | (CheckedOut, Active | Cancelled | Failed)
                | (Active, Submitted | Cancelled | Failed)
                | (Submitted, Validating | NeedsChanges | Rejected | Failed)
                | (Validating, Reviewing | NeedsChanges | Rejected | Failed)
                | (Reviewing, Approved | NeedsChanges | Rejected | Failed)
                | (NeedsChanges, Active | Submitted | Cancelled | Rejected)
                | (Approved, Integrating | Failed)
                | (Integrating, ReadyToRelease | Publishing | Failed)
                | (
                    ReadyToRelease,
                    NeedsChanges | Publishing | Cancelled | Failed
                )
                | (Publishing, Published | Failed)
                | (Published | Rejected | Cancelled | Failed, Closed)
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Closed)
    }

    pub fn revokes_lease(self) -> bool {
        matches!(
            self,
            Self::Published | Self::Rejected | Self::Cancelled | Self::Failed | Self::Closed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateCheckout {
    pub package: String,
    pub agent: String,
    #[serde(default)]
    pub consumers: Vec<String>,
    pub task: String,
    /// Client-generated capability used for checkout archive and submission access.
    pub lease_token: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutLease {
    pub checkout: CheckoutRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRequest {
    pub holder: String,
    pub lease_token: String,
    #[serde(default = "default_lease_seconds")]
    pub duration_seconds: i64,
    pub idempotency_key: String,
}

fn default_lease_seconds() -> i64 {
    8 * 60 * 60
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRecord {
    pub holder: String,
    pub token_digest: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowTransition {
    pub timestamp: DateTime<Utc>,
    pub actor: String,
    pub previous: Option<WorkflowState>,
    pub next: WorkflowState,
    pub commit: Option<String>,
    pub validation_result: Option<String>,
    pub reason: String,
    pub receipt_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutRecord {
    pub checkout_id: String,
    pub package: String,
    #[serde(default)]
    pub source_kind: SourceKind,
    pub agent: String,
    pub consumers: Vec<String>,
    pub task: String,
    pub state: WorkflowState,
    pub base_branch: Option<String>,
    pub base_commit: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub lease: Option<LeaseRecord>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub transitions: Vec<WorkflowTransition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionRequest {
    pub next: WorkflowState,
    pub actor: String,
    pub reason: String,
    pub commit: Option<String>,
    pub validation_result: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupRequest {
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    Checkout,
    SourcePrepared,
    Submission,
    Validation,
    Review,
    Integration,
    Release,
    Publication,
    LeaseAcquired,
    LeaseRenewed,
    LeaseReleased,
    Transition,
    Cleanup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowReceipt {
    pub receipt_id: String,
    pub kind: ReceiptKind,
    pub checkout_id: String,
    pub actor: String,
    pub timestamp: DateTime<Utc>,
    pub previous: Option<WorkflowState>,
    pub next: WorkflowState,
    pub commit: Option<String>,
    pub validation_result: Option<String>,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::WorkflowState;

    #[test]
    fn state_machine_allows_progress_and_rework_but_not_skips() {
        assert!(WorkflowState::Created.can_transition_to(WorkflowState::CheckedOut));
        assert!(WorkflowState::Reviewing.can_transition_to(WorkflowState::NeedsChanges));
        assert!(WorkflowState::NeedsChanges.can_transition_to(WorkflowState::Active));
        assert!(WorkflowState::ReadyToRelease.can_transition_to(WorkflowState::NeedsChanges));
        assert!(WorkflowState::Published.can_transition_to(WorkflowState::Closed));
        assert!(!WorkflowState::Created.can_transition_to(WorkflowState::Published));
        assert!(!WorkflowState::Closed.can_transition_to(WorkflowState::Active));
    }
}
