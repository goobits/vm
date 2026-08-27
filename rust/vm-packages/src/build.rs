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

    pub fn is_legacy_retryable_infrastructure_failure(&self) -> bool {
        self.failure_kind == Some(ToolBuildFailureKind::Build)
            && self
                .failure
                .as_deref()
                .is_some_and(|failure| failure.contains("Permission denied (os error 13)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_build(failure: &str) -> ToolBuildRecord {
        ToolBuildRecord {
            submission_id: "submission-1".into(),
            source_commit: "a".repeat(40),
            manifest_digest: "b".repeat(64),
            version: String::new(),
            artifacts: Vec::new(),
            failure: Some(failure.into()),
            failure_kind: Some(ToolBuildFailureKind::Build),
            actor: "tool-build-service".into(),
            completion_idempotency_key: Some("build-1".into()),
            completed_at: Utc::now(),
        }
    }

    #[test]
    fn only_legacy_permission_failures_are_directly_retryable() {
        assert!(failed_build("Permission denied (os error 13)")
            .is_legacy_retryable_infrastructure_failure());
        assert!(!failed_build("Cannot find module 'dependency'")
            .is_legacy_retryable_infrastructure_failure());
    }
}
