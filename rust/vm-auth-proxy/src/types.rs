//! Type definitions for auth proxy service

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// A stored secret with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
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
    pub fn new(encrypted_value: String, scope: SecretScope, description: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            encrypted_value,
            created_at: now,
            updated_at: now,
            scope,
            description,
        }
    }

    /// Update the secret value and timestamp
    pub fn update(&mut self, encrypted_value: String) {
        self.encrypted_value = encrypted_value;
        self.updated_at = Utc::now();
    }
}

/// Storage structure for persisted secrets
#[derive(Debug, Serialize, Deserialize)]
pub struct SecretStorage {
    /// Version of the storage format
    pub version: u32,
    /// Salt for key derivation
    pub salt: String,
    /// Stored secrets by name
    pub secrets: HashMap<String, Secret>,
    /// Authentication token for API access
    pub auth_token: Option<String>,
}

impl Default for SecretStorage {
    fn default() -> Self {
        Self {
            version: 1,
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
