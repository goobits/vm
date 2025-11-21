use crate::{auth::AuthenticatedUser, error::ApiResult, state::AppState};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::instrument;
use utoipa::IntoParams;
use vm_orchestrator::{Operation, OperationStatus, OperationType, WorkspaceFilters};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/operations", get(list_operations))
        .route("/api/v1/operations/{id}", get(get_operation))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct OperationsQuery {
    workspace_id: Option<String>,
    #[serde(rename = "type")]
    operation_type: Option<String>,
    status: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/operations",
    params(
        OperationsQuery
    ),
    responses(
        (status = 200, description = "List operations", body = Vec<Operation>)
    )
)]
#[instrument(skip(state), fields(user = %user.username, workspace_id = ?query.workspace_id))]
pub async fn list_operations(
    State(state): State<AppState>,
    Query(query): Query<OperationsQuery>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Vec<Operation>>> {
    // If workspace_id is provided, verify user owns it
    if let Some(ref workspace_id) = query.workspace_id {
        crate::auth::check_workspace_owner(&state.orchestrator, workspace_id, &user).await?;
    }

    // Parse operation_type from string
    let operation_type = query
        .operation_type
        .and_then(|s| serde_json::from_str::<OperationType>(&format!("\"{}\"", s)).ok());

    // Parse status from string
    let status = query
        .status
        .and_then(|s| serde_json::from_str::<OperationStatus>(&format!("\"{}\"", s)).ok());

    // Get operations
    let mut operations = state
        .orchestrator
        .get_operations(query.workspace_id.clone(), operation_type, status)
        .await?;

    // If no workspace_id was specified, filter to only show operations for user's workspaces
    if query.workspace_id.is_none() {
        let user_workspaces = state
            .orchestrator
            .list_workspaces(WorkspaceFilters {
                owner: Some(user.username.clone()),
                status: None,
            })
            .await?;

        let user_workspace_ids: std::collections::HashSet<_> =
            user_workspaces.iter().map(|w| w.id.clone()).collect();

        operations.retain(|op| user_workspace_ids.contains(&op.workspace_id));
    }

    Ok(Json(operations))
}

#[utoipa::path(
    get,
    path = "/api/v1/operations/{id}",
    params(
        ("id" = String, Path, description = "Operation ID")
    ),
    responses(
        (status = 200, description = "Get operation details", body = Operation)
    )
)]
#[instrument(skip(state), fields(user = %user.username, operation_id = %id))]
pub async fn get_operation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Extension(user): axum::Extension<AuthenticatedUser>,
) -> ApiResult<Json<Operation>> {
    let operation = state.orchestrator.get_operation(&id).await?;

    // Verify user owns the workspace associated with this operation
    crate::auth::check_workspace_owner(&state.orchestrator, &operation.workspace_id, &user).await?;

    Ok(Json(operation))
}
