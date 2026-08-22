use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{validate_label, validate_managed_id, PackageValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolActivationState {
    Queued,
    Activating,
    Waiting,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolActivationTargetState {
    Pending,
    Active,
    Deferred,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolActivationLease {
    pub worker: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolActivationTarget {
    pub target_id: String,
    pub environment: String,
    pub provider: String,
    pub initially_running: bool,
    pub state: ToolActivationTargetState,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolActivationRecord {
    pub activation_id: String,
    pub release_id: String,
    pub checkout_id: String,
    pub tool: String,
    pub version: String,
    pub source_commit: String,
    pub state: ToolActivationState,
    #[serde(default)]
    pub targets: Vec<ToolActivationTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<ToolActivationLease>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimToolActivationRequest {
    pub worker: String,
    #[serde(default = "default_lease_seconds")]
    pub lease_seconds: u64,
}

impl ClaimToolActivationRequest {
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        validate_label("tool activation worker", &self.worker)?;
        if !(30..=900).contains(&self.lease_seconds) {
            return Err(PackageValidationError::new(
                "tool activation lease must contain 30 to 900 seconds",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolActivationTargetPlan {
    pub target_id: String,
    pub environment: String,
    pub provider: String,
    pub initially_running: bool,
}

impl ToolActivationTargetPlan {
    fn validate(&self) -> Result<(), PackageValidationError> {
        validate_managed_id("tool activation target", &self.target_id)?;
        validate_label("tool activation environment", &self.environment)?;
        validate_label("tool activation provider", &self.provider)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanToolActivationRequest {
    pub worker: String,
    pub targets: Vec<ToolActivationTargetPlan>,
    pub idempotency_key: String,
}

impl PlanToolActivationRequest {
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        validate_label("tool activation worker", &self.worker)?;
        if self.targets.len() > 10_000 {
            return Err(PackageValidationError::new(
                "tool activation plan cannot exceed 10000 targets",
            ));
        }
        let mut ids = std::collections::BTreeSet::new();
        for target in &self.targets {
            target.validate()?;
            if !ids.insert(&target.target_id) {
                return Err(PackageValidationError::new(
                    "tool activation plan target IDs must be unique",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateToolActivationTargetRequest {
    pub worker: String,
    pub state: ToolActivationTargetState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub idempotency_key: String,
}

impl UpdateToolActivationTargetRequest {
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        validate_label("tool activation worker", &self.worker)?;
        if self.state == ToolActivationTargetState::Pending {
            return Err(PackageValidationError::new(
                "tool activation target updates must be active, deferred, or failed",
            ));
        }
        if self
            .error
            .as_ref()
            .is_some_and(|error| error.trim().is_empty() || error.len() > 4_000)
        {
            return Err(PackageValidationError::new(
                "tool activation target error must contain 1 to 4000 characters",
            ));
        }
        if self.state == ToolActivationTargetState::Failed && self.error.is_none() {
            return Err(PackageValidationError::new(
                "failed tool activation target requires an error",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinishToolActivationRequest {
    pub worker: String,
    pub idempotency_key: String,
}

impl FinishToolActivationRequest {
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        validate_label("tool activation worker", &self.worker)
    }
}

const fn default_lease_seconds() -> u64 {
    120
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_require_unique_valid_targets() {
        let target = ToolActivationTargetPlan {
            target_id: "target-1".into(),
            environment: "project-dev".into(),
            provider: "docker".into(),
            initially_running: true,
        };
        let request = PlanToolActivationRequest {
            worker: "worker-1".into(),
            targets: vec![target.clone(), target],
            idempotency_key: "plan-1".into(),
        };
        assert!(request.validate().is_err());
    }
}
