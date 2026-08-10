use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::PackageEcosystem;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterConsumer {
    pub name: String,
    pub repository: String,
    #[serde(default = "default_branch")]
    pub default_branch: String,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumerRecord {
    pub name: String,
    pub repository: String,
    pub default_branch: String,
    pub dependencies: BTreeMap<String, String>,
    pub registered_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumerUsage {
    pub consumer: String,
    pub version: String,
    pub pending_version: Option<String>,
    pub rollout_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDrift {
    pub package: String,
    pub latest_version: Option<String>,
    pub consumers: Vec<ConsumerUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRollout {
    pub package: String,
    pub version: String,
    pub consumer: String,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RolloutState {
    Created,
    Active,
    Validating,
    ReadyForReview,
    Failed,
    Cancelled,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolloutTransition {
    pub timestamp: DateTime<Utc>,
    pub actor: String,
    pub previous: Option<RolloutState>,
    pub next: RolloutState,
    pub commit: Option<String>,
    pub validation_result: Option<String>,
    pub reason: String,
    pub receipt_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolloutRecord {
    pub rollout_id: String,
    pub package: String,
    pub version: String,
    pub consumer: String,
    pub ecosystem: PackageEcosystem,
    pub state: RolloutState,
    pub base_commit: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub submitted_commit: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub transitions: Vec<RolloutTransition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolloutValidationRequest {
    pub passed: bool,
    pub actor: String,
    pub idempotency_key: String,
}

fn default_branch() -> String {
    "main".into()
}
