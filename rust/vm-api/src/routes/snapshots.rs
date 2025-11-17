use crate::{
    auth::{check_workspace_owner, AuthenticatedUser},
    error::ApiResult,
    state::AppState,
};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use vm_orchestrator::workspace::{CreateSnapshotRequest, Snapshot};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/workspaces/{id}/snapshots",
            get(list_snapshots).post(create_snapshot),
        )
        .route(
            "/api/v1/workspaces/{id}/snapshots/{snapshot_id}/restore",
            post(restore_snapshot),
        )
}

/// List all snapshots for a workspace
async fn list_snapshots(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Vec<Snapshot>>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &workspace_id, &user).await?;

    let snapshots = state.orchestrator.list_snapshots(&workspace_id).await?;
    Ok(Json(snapshots))
}

/// Create a new snapshot
async fn create_snapshot(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
    Json(req): Json<CreateSnapshotRequest>,
) -> ApiResult<Json<Snapshot>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &workspace_id, &user).await?;

    let snapshot = state
        .orchestrator
        .create_snapshot(&workspace_id, req)
        .await?;
    Ok(Json(snapshot))
}

/// Restore a workspace from a snapshot
async fn restore_snapshot(
    State(state): State<AppState>,
    Path((workspace_id, snapshot_id)): Path<(String, String)>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<serde_json::Value>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &workspace_id, &user).await?;

    state
        .orchestrator
        .restore_snapshot(&workspace_id, &snapshot_id)
        .await?;
    Ok(Json(
        serde_json::json!({ "message": "Snapshot restore initiated" }),
    ))
}
