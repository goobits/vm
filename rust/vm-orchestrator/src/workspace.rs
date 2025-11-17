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

    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: DateTime<Utc>,

    #[serde(serialize_with = "serialize_datetime")]
    pub updated_at: DateTime<Utc>,

    pub ttl_seconds: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(serialize_with = "serialize_optional_datetime")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub owner: String,
    pub repo_url: Option<String>,
    pub template: Option<String>,
    pub provider: Option<String>,
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: DateTime<Utc>,
    pub size_bytes: i64,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSnapshotRequest {
    pub name: String,
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

    /// Get a reference to the database pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
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
        .bind(
            connection_info.map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string())),
        )
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

    /// Update operation status
    pub async fn update_operation_status(
        &self,
        operation_id: &str,
        status: crate::operation::OperationStatus,
        error: Option<String>,
    ) -> Result<()> {
        let completed_at = if matches!(
            status,
            crate::operation::OperationStatus::Success | crate::operation::OperationStatus::Failed
        ) {
            Some(Utc::now().timestamp())
        } else {
            None
        };

        sqlx::query("UPDATE operations SET status = ?, completed_at = ?, error = ? WHERE id = ?")
            .bind(status)
            .bind(completed_at)
            .bind(error)
            .bind(operation_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Get a single operation by ID
    pub async fn get_operation(&self, id: &str) -> Result<crate::operation::Operation> {
        let row = sqlx::query_as::<_, OperationRow>("SELECT * FROM operations WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| OrchestratorError::NotFound(id.to_string()))?;

        Ok(row.into())
    }

    /// Get all operations with optional filters
    pub async fn get_operations(
        &self,
        workspace_id: Option<String>,
        operation_type: Option<crate::operation::OperationType>,
        status: Option<crate::operation::OperationStatus>,
    ) -> Result<Vec<crate::operation::Operation>> {
        let mut query = "SELECT * FROM operations WHERE 1=1".to_string();

        if workspace_id.is_some() {
            query.push_str(" AND workspace_id = ?");
        }
        if operation_type.is_some() {
            query.push_str(" AND operation_type = ?");
        }
        if status.is_some() {
            query.push_str(" AND status = ?");
        }

        query.push_str(" ORDER BY started_at DESC");

        let mut q = sqlx::query_as::<_, OperationRow>(&query);

        if let Some(wid) = &workspace_id {
            q = q.bind(wid);
        }
        if let Some(ot) = &operation_type {
            q = q.bind(ot);
        }
        if let Some(s) = &status {
            q = q.bind(s);
        }

        let rows = q.fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    /// List all snapshots for a workspace
    pub async fn list_snapshots(&self, workspace_id: &str) -> Result<Vec<Snapshot>> {
        let rows = sqlx::query_as::<_, SnapshotRow>(
            "SELECT * FROM snapshots WHERE workspace_id = ? ORDER BY created_at DESC",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    /// Create a new snapshot of a workspace
    pub async fn create_snapshot(
        &self,
        workspace_id: &str,
        req: CreateSnapshotRequest,
    ) -> Result<Snapshot> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        // Get workspace to verify it exists and has a provider_id
        let workspace = self.get_workspace(workspace_id).await?;
        let _provider_id = workspace.provider_id.clone().ok_or_else(|| {
            OrchestratorError::InvalidState("Workspace has no provider_id".to_string())
        })?;

        // Insert snapshot record first
        sqlx::query(
            r#"
            INSERT INTO snapshots (id, workspace_id, name, created_at, size_bytes, metadata)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(&req.name)
        .bind(now.timestamp())
        .bind(0) // Size will be updated after snapshot is created
        .bind(None::<String>)
        .execute(&self.pool)
        .await?;

        // Record operation as pending
        let operation_id = self
            .record_operation(
                workspace_id,
                crate::operation::OperationType::Snapshot,
                crate::operation::OperationStatus::Pending,
            )
            .await?;

        // Spawn background task to create actual snapshot
        let orchestrator = self.clone();
        let snapshot_id = id.clone();
        let snapshot_name = req.name.clone();
        let workspace_clone = workspace.clone();

        tokio::task::spawn(async move {
            // Update operation to running
            let _ = orchestrator
                .update_operation_status(
                    &operation_id,
                    crate::operation::OperationStatus::Running,
                    None,
                )
                .await;

            // Call provider snapshot in blocking context
            let snapshot_name_clone = snapshot_name.clone();
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<i64> {
                let config = build_workspace_config(&workspace_clone)?;
                let provider = vm_provider::get_provider(config)?;

                // Create snapshot request
                let snapshot_request = vm_provider::SnapshotRequest {
                    snapshot_name: snapshot_name_clone.clone(),
                    description: Some("Snapshot created via orchestrator".to_string()),
                    quiesce: false,
                };

                provider.snapshot(&snapshot_request)?;

                // Get the snapshot file size
                let snapshot_path = std::path::PathBuf::from("/tmp/vm-snapshots")
                    .join(format!("{}.tar", snapshot_name_clone));

                let size = std::fs::metadata(&snapshot_path)
                    .map(|m| m.len() as i64)
                    .unwrap_or(0);

                Ok(size)
            })
            .await;

            match result {
                Ok(Ok(size_bytes)) => {
                    // Success - update snapshot with real size
                    let _ = sqlx::query("UPDATE snapshots SET size_bytes = ? WHERE id = ?")
                        .bind(size_bytes)
                        .bind(&snapshot_id)
                        .execute(orchestrator.pool())
                        .await;

                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Success,
                            None,
                        )
                        .await;
                }
                Ok(Err(e)) => {
                    // Provider error
                    let error_msg = e.to_string();
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
                Err(e) => {
                    // Task join error
                    let error_msg = format!("Task failed: {}", e);
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
            }
        });

        Ok(Snapshot {
            id,
            workspace_id: workspace_id.to_string(),
            name: req.name,
            created_at: now,
            size_bytes: 0,
            metadata: None,
        })
    }

    /// Restore a workspace from a snapshot
    pub async fn restore_snapshot(&self, workspace_id: &str, snapshot_id: &str) -> Result<()> {
        // Verify snapshot exists and belongs to this workspace
        let snapshot_row = sqlx::query_as::<_, SnapshotRow>(
            "SELECT * FROM snapshots WHERE id = ? AND workspace_id = ?",
        )
        .bind(snapshot_id)
        .bind(workspace_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| {
            OrchestratorError::NotFound(format!(
                "Snapshot {} not found for workspace {}",
                snapshot_id, workspace_id
            ))
        })?;

        let snapshot: Snapshot = snapshot_row.into();

        // Get workspace
        let workspace = self.get_workspace(workspace_id).await?;

        // Record operation as pending
        let operation_id = self
            .record_operation(
                workspace_id,
                crate::operation::OperationType::SnapshotRestore,
                crate::operation::OperationStatus::Pending,
            )
            .await?;

        // Update workspace status to indicate restore in progress
        self.update_workspace_status(
            workspace_id,
            WorkspaceStatus::Creating,
            None,
            None,
            Some("Restoring from snapshot".to_string()),
        )
        .await?;

        // Spawn background task to restore snapshot
        let orchestrator = self.clone();
        let workspace_id_clone = workspace_id.to_string();
        let workspace_clone = workspace.clone();
        let snapshot_name = snapshot.name.clone();

        tokio::task::spawn(async move {
            // Update operation to running
            let _ = orchestrator
                .update_operation_status(
                    &operation_id,
                    crate::operation::OperationStatus::Running,
                    None,
                )
                .await;

            // Call provider restore in blocking context
            let snapshot_name_clone = snapshot_name.clone();
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
                let config = build_workspace_config(&workspace_clone)?;
                let provider = vm_provider::get_provider(config)?;

                // Create restore request
                let snapshot_path = std::path::PathBuf::from("/tmp/vm-snapshots")
                    .join(format!("{}.tar", snapshot_name_clone));

                let restore_request = vm_provider::SnapshotRestoreRequest {
                    snapshot_name: snapshot_name_clone.clone(),
                    snapshot_path,
                    force: true,
                };

                provider.restore_snapshot(&restore_request)?;

                // Get the new container name/ID
                // For now, we'll use the workspace name + "-restored" as the provider_id
                let project_name = workspace_clone.name.clone();
                let new_provider_id = format!("{}-restored", project_name);

                Ok(new_provider_id)
            })
            .await;

            match result {
                Ok(Ok(new_provider_id)) => {
                    // Success - update workspace with new provider_id and mark as running
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id_clone,
                            WorkspaceStatus::Running,
                            Some(new_provider_id),
                            None, // Connection info would need to be regenerated
                            None,
                        )
                        .await;

                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Success,
                            None,
                        )
                        .await;
                }
                Ok(Err(e)) => {
                    // Provider error
                    let error_msg = e.to_string();
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id_clone,
                            WorkspaceStatus::Failed,
                            None,
                            None,
                            Some(error_msg.clone()),
                        )
                        .await;

                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
                Err(e) => {
                    // Task join error
                    let error_msg = format!("Task failed: {}", e);
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id_clone,
                            WorkspaceStatus::Failed,
                            None,
                            None,
                            Some(error_msg.clone()),
                        )
                        .await;

                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
            }
        });

        Ok(())
    }

    /// Start a stopped workspace
    pub async fn start_workspace(&self, id: &str) -> Result<Workspace> {
        let workspace = self.get_workspace(id).await?;

        // Validate workspace has provider_id
        let provider_id = workspace.provider_id.clone().ok_or_else(|| {
            OrchestratorError::InvalidState("Workspace has no provider_id".to_string())
        })?;

        // Record operation as pending
        let operation_id = self
            .record_operation(
                id,
                crate::operation::OperationType::Start,
                crate::operation::OperationStatus::Pending,
            )
            .await?;

        // Spawn task to perform actual start
        let orchestrator = self.clone();
        let workspace_id = id.to_string();
        let workspace_clone = workspace.clone();
        let saved_provider_id = workspace.provider_id.clone();
        let saved_connection_info = workspace.connection_info.clone();

        tokio::task::spawn(async move {
            // Update operation to running
            let _ = orchestrator
                .update_operation_status(
                    &operation_id,
                    crate::operation::OperationStatus::Running,
                    None,
                )
                .await;

            // Call provider start in blocking context
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let config = build_workspace_config(&workspace_clone)?;
                let provider = vm_provider::get_provider(config)?;
                provider.start(Some(&provider_id))?;
                Ok(())
            })
            .await;

            match result {
                Ok(Ok(())) => {
                    // Success - update workspace and operation
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id,
                            WorkspaceStatus::Running,
                            saved_provider_id.clone(),
                            saved_connection_info.clone(),
                            None,
                        )
                        .await;
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Success,
                            None,
                        )
                        .await;
                }
                Ok(Err(e)) => {
                    // Provider error
                    let error_msg = e.to_string();
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id,
                            WorkspaceStatus::Failed,
                            saved_provider_id.clone(),
                            None,
                            Some(error_msg.clone()),
                        )
                        .await;
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
                Err(e) => {
                    // Task join error
                    let error_msg = format!("Task failed: {}", e);
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id,
                            WorkspaceStatus::Failed,
                            saved_provider_id.clone(),
                            None,
                            Some(error_msg.clone()),
                        )
                        .await;
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
            }
        });

        self.get_workspace(id).await
    }

    /// Stop a running workspace
    pub async fn stop_workspace(&self, id: &str) -> Result<Workspace> {
        let workspace = self.get_workspace(id).await?;

        // Validate workspace has provider_id
        let provider_id = workspace.provider_id.clone().ok_or_else(|| {
            OrchestratorError::InvalidState("Workspace has no provider_id".to_string())
        })?;

        // Record operation as pending
        let operation_id = self
            .record_operation(
                id,
                crate::operation::OperationType::Stop,
                crate::operation::OperationStatus::Pending,
            )
            .await?;

        // Spawn task to perform actual stop
        let orchestrator = self.clone();
        let workspace_id = id.to_string();
        let workspace_clone = workspace.clone();
        let saved_provider_id = workspace.provider_id.clone();

        tokio::task::spawn(async move {
            // Update operation to running
            let _ = orchestrator
                .update_operation_status(
                    &operation_id,
                    crate::operation::OperationStatus::Running,
                    None,
                )
                .await;

            // Call provider stop in blocking context
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let config = build_workspace_config(&workspace_clone)?;
                let provider = vm_provider::get_provider(config)?;
                provider.stop(Some(&provider_id))?;
                Ok(())
            })
            .await;

            match result {
                Ok(Ok(())) => {
                    // Success - update workspace and operation
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id,
                            WorkspaceStatus::Stopped,
                            saved_provider_id.clone(),
                            None, // Clear connection info when stopped
                            None,
                        )
                        .await;
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Success,
                            None,
                        )
                        .await;
                }
                Ok(Err(e)) => {
                    // Provider error - update operation only (don't mark workspace as failed)
                    let error_msg = e.to_string();
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
                Err(e) => {
                    // Task join error - update operation only
                    let error_msg = format!("Task failed: {}", e);
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
            }
        });

        self.get_workspace(id).await
    }

    /// Restart a workspace
    pub async fn restart_workspace(&self, id: &str) -> Result<Workspace> {
        let workspace = self.get_workspace(id).await?;

        // Validate workspace has provider_id
        let provider_id = workspace.provider_id.clone().ok_or_else(|| {
            OrchestratorError::InvalidState("Workspace has no provider_id".to_string())
        })?;

        // Record operation as pending
        let operation_id = self
            .record_operation(
                id,
                crate::operation::OperationType::Restart,
                crate::operation::OperationStatus::Pending,
            )
            .await?;

        // Spawn task to perform actual restart
        let orchestrator = self.clone();
        let workspace_id = id.to_string();
        let workspace_clone = workspace.clone();
        let saved_provider_id = workspace.provider_id.clone();
        let saved_connection_info = workspace.connection_info.clone();

        tokio::task::spawn(async move {
            // Update operation to running
            let _ = orchestrator
                .update_operation_status(
                    &operation_id,
                    crate::operation::OperationStatus::Running,
                    None,
                )
                .await;

            // Call provider stop then start in blocking context
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let config = build_workspace_config(&workspace_clone)?;
                let provider = vm_provider::get_provider(config)?;
                provider.stop(Some(&provider_id))?;
                provider.start(Some(&provider_id))?;
                Ok(())
            })
            .await;

            match result {
                Ok(Ok(())) => {
                    // Success - update workspace and operation
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id,
                            WorkspaceStatus::Running,
                            saved_provider_id.clone(),
                            saved_connection_info.clone(),
                            None,
                        )
                        .await;
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Success,
                            None,
                        )
                        .await;
                }
                Ok(Err(e)) => {
                    // Provider error
                    let error_msg = e.to_string();
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id,
                            WorkspaceStatus::Failed,
                            saved_provider_id.clone(),
                            None,
                            Some(error_msg.clone()),
                        )
                        .await;
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
                Err(e) => {
                    // Task join error
                    let error_msg = format!("Task failed: {}", e);
                    let _ = orchestrator
                        .update_workspace_status(
                            &workspace_id,
                            WorkspaceStatus::Failed,
                            saved_provider_id.clone(),
                            None,
                            Some(error_msg.clone()),
                        )
                        .await;
                    let _ = orchestrator
                        .update_operation_status(
                            &operation_id,
                            crate::operation::OperationStatus::Failed,
                            Some(error_msg),
                        )
                        .await;
                }
            }
        });

        self.get_workspace(id).await
    }
}

/// Build a VmConfig from workspace metadata for provider operations
fn build_workspace_config(workspace: &Workspace) -> anyhow::Result<vm_config::config::VmConfig> {
    let mut config = vm_config::config::VmConfig::default();

    config.project = Some(vm_config::config::ProjectConfig {
        name: Some(workspace.name.clone()),
        ..Default::default()
    });

    config.provider = Some(workspace.provider.clone());

    Ok(config)
}

// Internal row types for sqlx
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

#[derive(sqlx::FromRow)]
struct OperationRow {
    id: String,
    workspace_id: String,
    operation_type: crate::operation::OperationType,
    status: crate::operation::OperationStatus,
    started_at: i64,
    completed_at: Option<i64>,
    error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SnapshotRow {
    id: String,
    workspace_id: String,
    name: String,
    created_at: i64,
    size_bytes: i64,
    metadata: Option<String>,
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
            created_at: DateTime::from_timestamp(row.created_at, 0).unwrap_or(DateTime::UNIX_EPOCH),
            updated_at: DateTime::from_timestamp(row.updated_at, 0).unwrap_or(DateTime::UNIX_EPOCH),
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

impl From<OperationRow> for crate::operation::Operation {
    fn from(row: OperationRow) -> Self {
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            operation_type: row.operation_type,
            status: row.status,
            started_at: DateTime::from_timestamp(row.started_at, 0).unwrap_or(DateTime::UNIX_EPOCH),
            completed_at: row
                .completed_at
                .and_then(|ts| DateTime::from_timestamp(ts, 0)),
            error: row.error,
        }
    }
}

impl From<SnapshotRow> for Snapshot {
    fn from(row: SnapshotRow) -> Self {
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            name: row.name,
            created_at: DateTime::from_timestamp(row.created_at, 0).unwrap_or(DateTime::UNIX_EPOCH),
            size_bytes: row.size_bytes,
            metadata: row.metadata.and_then(|s| serde_json::from_str(&s).ok()),
        }
    }
}
// Serialize DateTime as RFC 3339 / ISO 8601 string
fn serialize_datetime<S>(dt: &DateTime<Utc>, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&dt.to_rfc3339())
}

fn serialize_optional_datetime<S>(
    dt: &Option<DateTime<Utc>>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match dt {
        Some(dt) => serializer.serialize_str(&dt.to_rfc3339()),
        None => serializer.serialize_none(),
    }
}
