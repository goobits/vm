use crate::error::{OrchestratorError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub repo_url: Option<String>,
    pub template: Option<String>,
    pub provider: String,
    pub status: WorkspaceStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub ttl_seconds: Option<i64>,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
    pub provider_id: Option<String>,
    pub connection_info: Option<serde_json::Value>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
pub enum WorkspaceStatus {
    Creating,
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub owner: String,
    pub repo_url: Option<String>,
    pub template: Option<String>,
    pub provider: Option<String>,
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceFilters {
    pub owner: Option<String>,
    pub status: Option<WorkspaceStatus>,
}

#[derive(Clone)]
pub struct WorkspaceOrchestrator {
    pool: SqlitePool,
}

impl WorkspaceOrchestrator {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new workspace
    pub async fn create_workspace(&self, req: CreateWorkspaceRequest) -> Result<Workspace> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let provider = req.provider.unwrap_or_else(|| "docker".to_string());

        let expires_at = req
            .ttl_seconds
            .map(|ttl| now + chrono::Duration::seconds(ttl));

        sqlx::query(
            r#"
            INSERT INTO workspaces (id, name, owner, repo_url, template, provider, status, created_at, updated_at, ttl_seconds, expires_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&id)
        .bind(&req.name)
        .bind(&req.owner)
        .bind(&req.repo_url)
        .bind(&req.template)
        .bind(&provider)
        .bind(WorkspaceStatus::Creating)
        .bind(now.timestamp())
        .bind(now.timestamp())
        .bind(req.ttl_seconds)
        .bind(expires_at.map(|dt| dt.timestamp()))
        .execute(&self.pool)
        .await?;

        // Record the create operation
        self.record_operation(
            &id,
            crate::operation::OperationType::Create,
            crate::operation::OperationStatus::Pending,
        )
        .await?;

        self.get_workspace(&id).await
    }

    /// List workspaces with optional filters
    pub async fn list_workspaces(&self, filters: WorkspaceFilters) -> Result<Vec<Workspace>> {
        let mut query = "SELECT * FROM workspaces WHERE 1=1".to_string();

        if filters.owner.is_some() {
            query.push_str(" AND owner = ?");
        }
        if filters.status.is_some() {
            query.push_str(" AND status = ?");
        }

        query.push_str(" ORDER BY created_at DESC");

        let mut q = sqlx::query_as::<_, WorkspaceRow>(&query);

        if let Some(owner) = &filters.owner {
            q = q.bind(owner);
        }
        if let Some(status) = &filters.status {
            q = q.bind(status);
        }

        let rows = q.fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    /// Get a single workspace by ID
    pub async fn get_workspace(&self, id: &str) -> Result<Workspace> {
        let row = sqlx::query_as::<_, WorkspaceRow>("SELECT * FROM workspaces WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| OrchestratorError::NotFound(id.to_string()))?;

        Ok(row.into())
    }

    /// Delete a workspace and destroy its VM
    pub async fn delete_workspace(&self, id: &str) -> Result<()> {
        // Get workspace first to get provider_id
        let workspace = self.get_workspace(id).await?;

        // Record delete operation
        self.record_operation(
            id,
            crate::operation::OperationType::Delete,
            crate::operation::OperationStatus::Running,
        )
        .await
        .ok();

        // If it has a provider_id, destroy the VM
        if let Some(provider_id) = workspace.provider_id {
            tracing::info!("Destroying VM for workspace {}: {}", id, provider_id);

            let provider_name = workspace.provider.clone();
            tokio::task::spawn_blocking(move || {
                let config = vm_config::config::VmConfig {
                    provider: Some(provider_name.clone()),
                    project: Some(vm_config::config::ProjectConfig {
                        name: Some(provider_id.clone()),
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                if let Ok(provider) = vm_provider::get_provider(config) {
                    provider.destroy(Some(&provider_id)).ok(); // Best effort
                }
            })
            .await
            .ok();
        }

        // Delete from database
        let result = sqlx::query("DELETE FROM workspaces WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(OrchestratorError::NotFound(id.to_string()));
        }

        Ok(())
    }

    /// Get workspaces that have expired (for janitor cleanup)
    pub async fn get_expired_workspaces(&self) -> Result<Vec<Workspace>> {
        let now = Utc::now().timestamp();

        let rows = sqlx::query_as::<_, WorkspaceRow>(
            "SELECT * FROM workspaces WHERE expires_at IS NOT NULL AND expires_at < ?",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    /// Update workspace status and related fields
    pub async fn update_workspace_status(
        &self,
        id: &str,
        status: WorkspaceStatus,
        provider_id: Option<String>,
        connection_info: Option<serde_json::Value>,
        error_message: Option<String>,
    ) -> Result<()> {
        let now = Utc::now().timestamp();

        sqlx::query(
            "UPDATE workspaces
             SET status = ?, updated_at = ?, provider_id = ?, connection_info = ?, error_message = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(now)
        .bind(provider_id)
        .bind(connection_info.map(|v| serde_json::to_string(&v).unwrap()))
        .bind(error_message)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get all workspaces with a specific status
    pub async fn get_workspaces_by_status(
        &self,
        status: WorkspaceStatus,
    ) -> Result<Vec<Workspace>> {
        let rows = sqlx::query_as::<_, WorkspaceRow>("SELECT * FROM workspaces WHERE status = ?")
            .bind(status)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    /// Record an operation for tracking
    pub async fn record_operation(
        &self,
        workspace_id: &str,
        operation_type: crate::operation::OperationType,
        status: crate::operation::OperationStatus,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO operations (id, workspace_id, operation_type, status, started_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(operation_type)
        .bind(status)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }
}

// Internal row type for sqlx
#[derive(sqlx::FromRow)]
struct WorkspaceRow {
    id: String,
    name: String,
    owner: String,
    repo_url: Option<String>,
    template: Option<String>,
    provider: String,
    status: WorkspaceStatus,
    created_at: i64,
    updated_at: i64,
    ttl_seconds: Option<i64>,
    expires_at: Option<i64>,
    metadata: Option<String>,
    provider_id: Option<String>,
    connection_info: Option<String>,
    error_message: Option<String>,
}

impl From<WorkspaceRow> for Workspace {
    fn from(row: WorkspaceRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            owner: row.owner,
            repo_url: row.repo_url,
            template: row.template,
            provider: row.provider,
            status: row.status,
            created_at: DateTime::from_timestamp(row.created_at, 0).unwrap(),
            updated_at: DateTime::from_timestamp(row.updated_at, 0).unwrap(),
            ttl_seconds: row.ttl_seconds,
            expires_at: row
                .expires_at
                .and_then(|ts| DateTime::from_timestamp(ts, 0)),
            metadata: row.metadata.and_then(|s| serde_json::from_str(&s).ok()),
            provider_id: row.provider_id,
            connection_info: row
                .connection_info
                .and_then(|s| serde_json::from_str(&s).ok()),
            error_message: row.error_message,
        }
    }
}
