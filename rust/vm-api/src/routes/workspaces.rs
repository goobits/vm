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
use tracing::instrument;
use vm_orchestrator::{CreateWorkspaceRequest, Workspace, WorkspaceFilters};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route(
            "/api/v1/workspaces/{id}",
            get(get_workspace).delete(delete_workspace),
        )
        .route("/api/v1/workspaces/{id}/start", post(start_workspace))
        .route("/api/v1/workspaces/{id}/stop", post(stop_workspace))
        .route("/api/v1/workspaces/{id}/restart", post(restart_workspace))
}

#[instrument(skip(state), fields(user = %user.username, workspace_name = %req.name))]
async fn create_workspace(
    State(state): State<AppState>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
    Json(mut req): Json<CreateWorkspaceRequest>,
) -> ApiResult<Json<Workspace>> {
    // Override owner with authenticated user
    req.owner = user.username;

    let workspace = state.orchestrator.create_workspace(req).await?;

    Ok(Json(workspace))
}

#[instrument(skip(state), fields(user = %user.username))]
async fn list_workspaces(
    State(state): State<AppState>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Vec<Workspace>>> {
    let filters = WorkspaceFilters {
        owner: Some(user.username),
        status: None,
    };

    let workspaces = state.orchestrator.list_workspaces(filters).await?;

    Ok(Json(workspaces))
}

#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
async fn get_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Workspace>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    let workspace = state.orchestrator.get_workspace(&id).await?;

    Ok(Json(workspace))
}

#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
async fn delete_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<serde_json::Value>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    state.orchestrator.delete_workspace(&id).await?;

    Ok(Json(serde_json::json!({ "message": "Workspace deleted" })))
}

#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
async fn start_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Workspace>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    let workspace = state.orchestrator.start_workspace(&id).await?;
    Ok(Json(workspace))
}

#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
async fn stop_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Workspace>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    let workspace = state.orchestrator.stop_workspace(&id).await?;
    Ok(Json(workspace))
}

#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
async fn restart_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Workspace>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    let workspace = state.orchestrator.restart_workspace(&id).await?;
    Ok(Json(workspace))
}
