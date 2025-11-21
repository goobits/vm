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

#[utoipa::path(
    post,
    path = "/api/v1/workspaces",
    request_body = CreateWorkspaceRequest,
    responses(
        (status = 200, description = "Create a new workspace", body = Workspace)
    )
)]
#[instrument(skip(state), fields(user = %user.username, workspace_name = %req.name))]
pub async fn create_workspace(
    State(state): State<AppState>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
    Json(mut req): Json<CreateWorkspaceRequest>,
) -> ApiResult<Json<Workspace>> {
    // Override owner with authenticated user
    req.owner = user.username;

    let workspace = state.orchestrator.create_workspace(req).await?;

    Ok(Json(workspace))
}

#[utoipa::path(
    get,
    path = "/api/v1/workspaces",
    responses(
        (status = 200, description = "List workspaces", body = Vec<Workspace>)
    )
)]
#[instrument(skip(state), fields(user = %user.username))]
pub async fn list_workspaces(
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

#[utoipa::path(
    get,
    path = "/api/v1/workspaces/{id}",
    params(
        ("id" = String, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Get workspace details", body = Workspace)
    )
)]
#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
pub async fn get_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Workspace>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    let workspace = state.orchestrator.get_workspace(&id).await?;

    Ok(Json(workspace))
}

#[utoipa::path(
    delete,
    path = "/api/v1/workspaces/{id}",
    params(
        ("id" = String, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Delete workspace", body = serde_json::Value)
    )
)]
#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
pub async fn delete_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<serde_json::Value>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    state.orchestrator.delete_workspace(&id).await?;

    Ok(Json(serde_json::json!({ "message": "Workspace deleted" })))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{id}/start",
    params(
        ("id" = String, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Start workspace", body = Workspace)
    )
)]
#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
pub async fn start_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Workspace>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    let workspace = state.orchestrator.start_workspace(&id).await?;
    Ok(Json(workspace))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{id}/stop",
    params(
        ("id" = String, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Stop workspace", body = Workspace)
    )
)]
#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
pub async fn stop_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Workspace>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    let workspace = state.orchestrator.stop_workspace(&id).await?;
    Ok(Json(workspace))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{id}/restart",
    params(
        ("id" = String, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Restart workspace", body = Workspace)
    )
)]
#[instrument(skip(state), fields(user = %user.username, workspace_id = %id))]
pub async fn restart_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Workspace>> {
    // Check ownership
    check_workspace_owner(&state.orchestrator, &id, &user).await?;

    let workspace = state.orchestrator.restart_workspace(&id).await?;
    Ok(Json(workspace))
}
