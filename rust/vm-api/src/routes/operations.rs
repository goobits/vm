use crate::{error::ApiResult, state::AppState};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use vm_orchestrator::{Operation, OperationStatus, OperationType};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/operations", get(list_operations))
        .route("/api/v1/operations/{id}", get(get_operation))
}

#[derive(Debug, Deserialize)]
struct OperationsQuery {
    workspace_id: Option<String>,
    #[serde(rename = "type")]
    operation_type: Option<String>,
    status: Option<String>,
}

async fn list_operations(
    State(state): State<AppState>,
    Query(query): Query<OperationsQuery>,
) -> ApiResult<Json<Vec<Operation>>> {
    // Parse operation_type from string
    let operation_type = query
        .operation_type
        .and_then(|s| serde_json::from_str::<OperationType>(&format!("\"{}\"", s)).ok());

    // Parse status from string
    let status = query
        .status
        .and_then(|s| serde_json::from_str::<OperationStatus>(&format!("\"{}\"", s)).ok());

    let operations = state
        .orchestrator
        .get_operations(query.workspace_id, operation_type, status)
        .await?;

    Ok(Json(operations))
}

async fn get_operation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Operation>> {
    let operation = state.orchestrator.get_operation(&id).await?;

    Ok(Json(operation))
}
