use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::WorkflowState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeginReleaseRequest {
    pub version: String,
    pub tag: String,
    pub source_commit: String,
    pub artifact_digest: String,
    pub source_pushed: bool,
    pub expected_registries: Vec<String>,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationRequest {
    pub registry: String,
    pub artifact_digest: String,
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteReleaseRequest {
    pub actor: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationRecord {
    pub registry: String,
    pub artifact_digest: String,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseRecord {
    pub release_id: String,
    pub submission_id: String,
    pub checkout_id: String,
    pub package: String,
    pub version: String,
    pub source_repository: String,
    pub source_commit: String,
    pub tag: String,
    pub artifact_digest: String,
    pub source_pushed: bool,
    pub expected_registries: Vec<String>,
    pub publications: Vec<PublicationRecord>,
    pub state: WorkflowState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
