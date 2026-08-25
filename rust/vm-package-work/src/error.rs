use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub type WorkResult<T> = Result<T, WorkError>;

#[derive(Debug, thiserror::Error)]
pub enum WorkError {
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for WorkError {
    fn into_response(self) -> Response {
        let (status, error_code) = match &self {
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        if status.is_server_error() {
            tracing::error!(
                operation = "http_request",
                error_code,
                status_code = status.as_u16(),
                error = %self,
                "package workflow request failed"
            );
        } else if status == StatusCode::UNAUTHORIZED {
            tracing::warn!(
                operation = "http_request",
                error_code,
                status_code = status.as_u16(),
                "package workflow request rejected"
            );
        } else {
            tracing::debug!(
                operation = "http_request",
                error_code,
                status_code = status.as_u16(),
                error = %self,
                "package workflow request rejected"
            );
        }
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

impl From<std::io::Error> for WorkError {
    fn from(error: std::io::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<serde_json::Error> for WorkError {
    fn from(error: serde_json::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<vm_packages::PackageValidationError> for WorkError {
    fn from(error: vm_packages::PackageValidationError) -> Self {
        Self::Invalid(error.to_string())
    }
}
