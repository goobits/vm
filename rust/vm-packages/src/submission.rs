use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{validate_label, validate_tool_target, PackageValidationError, WorkflowState};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolBuildPhase {
    Preparing,
    RestoringDependencies,
    Building,
    Staging,
    Complete,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildProgress {
    pub attempt: String,
    pub phase: ToolBuildPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub actor: String,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateToolBuildProgressRequest {
    pub attempt: String,
    pub phase: ToolBuildPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub actor: String,
    pub idempotency_key: String,
}

impl UpdateToolBuildProgressRequest {
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        validate_label("tool build attempt", &self.attempt)?;
        validate_label("tool build progress actor", &self.actor)?;
        if matches!(
            self.phase,
            ToolBuildPhase::Complete | ToolBuildPhase::Failed
        ) {
            return Err(PackageValidationError::new(
                "terminal tool build progress is recorded by build completion",
            ));
        }
        if matches!(self.phase, ToolBuildPhase::Building) && self.target.is_none() {
            return Err(PackageValidationError::new(
                "building progress requires a tool target",
            ));
        }
        if let Some(target) = &self.target {
            validate_tool_target(target)?;
        }
        Ok(())
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_progress: Option<ToolBuildProgress>,
    #[serde(default)]
    pub release_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::{ToolBuildPhase, UpdateToolBuildProgressRequest};

    fn request(phase: ToolBuildPhase, target: Option<&str>) -> UpdateToolBuildProgressRequest {
        UpdateToolBuildProgressRequest {
            attempt: "build-attempt-1".into(),
            phase,
            target: target.map(str::to_string),
            actor: "tool-build-service".into(),
            idempotency_key: "build-progress-test".into(),
        }
    }

    #[test]
    fn build_progress_requires_a_target_only_while_building() {
        assert!(request(ToolBuildPhase::Preparing, None).validate().is_ok());
        assert!(request(ToolBuildPhase::Building, None).validate().is_err());
        assert!(request(ToolBuildPhase::Building, Some("linux-arm64"))
            .validate()
            .is_ok());
    }

    #[test]
    fn only_build_completion_may_record_terminal_progress() {
        assert!(request(ToolBuildPhase::Complete, None).validate().is_err());
        assert!(request(ToolBuildPhase::Failed, None).validate().is_err());
    }
}
