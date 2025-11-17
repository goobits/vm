use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Forbidden(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<vm_orchestrator::error::OrchestratorError> for ApiError {
    fn from(err: vm_orchestrator::error::OrchestratorError) -> Self {
        match err {
            vm_orchestrator::error::OrchestratorError::NotFound(id) => {
                ApiError::NotFound(format!("Workspace not found: {}", id))
            }
            vm_orchestrator::error::OrchestratorError::InvalidInput(msg) => {
                ApiError::BadRequest(msg)
            }
            _ => ApiError::Internal(err.to_string()),
        }
    }
}
