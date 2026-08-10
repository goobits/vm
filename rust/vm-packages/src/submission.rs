use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::WorkflowState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationResult {
    pub package: CheckOutcome,
    pub consumers: BTreeMap<String, CheckOutcome>,
    pub actor: String,
    pub timestamp: DateTime<Utc>,
}

impl ValidationResult {
    pub fn passed(&self) -> bool {
        self.package == CheckOutcome::Passed
            && self
                .consumers
                .values()
                .all(|outcome| *outcome == CheckOutcome::Passed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationRequest {
    pub package: CheckOutcome,
    pub consumers: BTreeMap<String, CheckOutcome>,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approve,
    Reject,
    NeedsChanges,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionRecommendation {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicApiDiff {
    #[serde(default)]
    pub changed_paths: Vec<String>,
    pub potentially_breaking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationReview {
    pub decision: ReviewDecision,
    pub recommended_version: VersionRecommendation,
    pub api_diff: PublicApiDiff,
    pub reason: String,
    #[serde(default)]
    pub required_followups: Vec<String>,
    pub merge_strategy: String,
    pub reviewer: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub decision: ReviewDecision,
    pub recommended_version: VersionRecommendation,
    pub api_diff: PublicApiDiff,
    pub reason: String,
    #[serde(default)]
    pub required_followups: Vec<String>,
    pub merge_strategy: String,
    pub reviewer: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationRecord {
    pub canonical_commit: String,
    pub integration_commit: String,
    pub strategy: String,
    pub worktree: String,
    pub validation: Option<ValidationResult>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationRequest {
    pub actor: String,
    pub strategy: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionRecord {
    pub submission_id: String,
    pub checkout_id: String,
    pub package: String,
    pub branch: String,
    pub base_commit: String,
    pub submitted_commit: String,
    pub diff_digest: String,
    pub state: WorkflowState,
    pub validation: Option<ValidationResult>,
    pub review: Option<IntegrationReview>,
    pub integration: Option<IntegrationRecord>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
