//! Type definitions for auth proxy service

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const SECRET_STORAGE_VERSION: u32 = 3;

/// Scope of secret access
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum SecretScope {
    /// Available to all VMs globally
    #[default]
    Global,
    /// Available to specific project VMs only
    Project(String),
    /// Available to specific VM instance only
    Instance(String),
}

impl SecretScope {
    pub fn parse(value: &str) -> Option<Self> {
        if value == "global" {
            return Some(Self::Global);
        }
        if let Some(project) = value
            .strip_prefix("project:")
            .filter(|name| !name.is_empty())
        {
            return Some(Self::Project(project.to_string()));
        }
        value
            .strip_prefix("instance:")
            .filter(|name| !name.is_empty())
            .map(|instance| Self::Instance(instance.to_string()))
    }
}

/// A stored secret with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    /// The name within its access scope
    #[serde(default)]
    pub name: String,
    /// Encrypted secret value
    pub encrypted_value: String,
    /// When the secret was created
    pub created_at: DateTime<Utc>,
    /// When the secret was last modified
    pub updated_at: DateTime<Utc>,
    /// Access scope for this secret
    pub scope: SecretScope,
    /// Optional description
    pub description: Option<String>,
}

impl Secret {
    /// Create a new secret with encrypted value
    pub fn new(
        name: String,
        encrypted_value: String,
        scope: SecretScope,
        description: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            name,
            encrypted_value,
            created_at: now,
            updated_at: now,
            scope,
            description,
        }
    }
}

/// Storage structure for persisted secrets
#[derive(Debug, Serialize, Deserialize)]
pub struct SecretStorage {
    /// Version of the storage format
    pub version: u32,
    /// Salt for key derivation
    pub salt: String,
    /// Stored secrets by scoped identity
    pub secrets: HashMap<String, Secret>,
    /// Authentication token for API access
    pub auth_token: Option<String>,
}

impl Default for SecretStorage {
    fn default() -> Self {
        Self {
            version: SECRET_STORAGE_VERSION,
            salt: String::new(),
            secrets: HashMap::new(),
            auth_token: None,
        }
    }
}

/// Request structure for adding/updating secrets
#[derive(Debug, Serialize, Deserialize)]
pub struct SecretRequest {
    /// Secret value to store
    pub value: String,
    /// Access scope (default: Global)
    #[serde(default)]
    pub scope: SecretScope,
    /// Optional description
    pub description: Option<String>,
}

/// Response structure for secret operations
#[derive(Debug, Serialize)]
pub struct SecretResponse {
    /// Secret name
    pub name: String,
    /// Whether operation was successful
    pub success: bool,
    /// Optional message
    pub message: Option<String>,
}

/// Response structure for listing secrets
#[derive(Debug, Serialize, Deserialize)]
pub struct SecretListResponse {
    /// List of secret summaries (no values)
    pub secrets: Vec<SecretSummary>,
    /// Total count
    pub total: usize,
}

/// Summary of a secret for listing (no sensitive data)
#[derive(Debug, Serialize, Deserialize)]
pub struct SecretSummary {
    /// Secret name
    pub name: String,
    /// When created
    pub created_at: DateTime<Utc>,
    /// When last updated
    pub updated_at: DateTime<Utc>,
    /// Access scope
    pub scope: SecretScope,
    /// Optional description
    pub description: Option<String>,
}

/// Environment variables response for VM integration
#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentResponse {
    /// Environment variables as key-value pairs
    pub env_vars: HashMap<String, String>,
    /// VM name this response is for
    pub vm_name: String,
    /// Project name if applicable
    pub project_name: Option<String>,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Number of stored secrets
    pub secret_count: usize,
    /// Service version
    pub version: String,
    /// Uptime in seconds
    pub uptime_seconds: u64,
}
