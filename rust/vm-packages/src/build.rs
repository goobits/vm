use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolBuildFailureKind {
    Build,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildArtifact {
    pub target: String,
    pub artifact_digest: String,
    pub size_bytes: u64,
    pub links: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteToolBuildRequest {
    pub source_commit: String,
    pub manifest_digest: String,
    pub version: String,
    #[serde(default)]
    pub artifacts: Vec<ToolBuildArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<ToolBuildFailureKind>,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryToolBuildRequest {
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildRecord {
    pub submission_id: String,
    pub source_commit: String,
    pub manifest_digest: String,
    pub version: String,
    pub artifacts: Vec<ToolBuildArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<ToolBuildFailureKind>,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_idempotency_key: Option<String>,
    pub completed_at: DateTime<Utc>,
}

impl ToolBuildRecord {
    pub fn succeeded(&self) -> bool {
        self.failure.is_none() && !self.artifacts.is_empty()
    }
}
