//! # Error Handling and Response Types
//!
//! This module provides comprehensive error handling for the package registry server.
//! It defines standardized error types, response formats, and HTTP status code mappings
//! to ensure consistent error handling across all API endpoints.
//!
//! ## Key Types
//!
//! - [`AppError`]: Main error enum covering all possible application errors
//! - [`ApiErrorResponse`]: Standardized JSON error response format
//! - [`ErrorCode`]: Machine-readable error classification
//! - [`AppResult<T>`]: Convenience type alias for Results using `AppError`
//!
//! ## Error Response Format
//!
//! All API errors are returned in a consistent JSON format:
//!
//! ```json
//! {
//!   "error": "Human-readable error message",
//!   "code": "machine_readable_error_code",
//!   "details": {...},  // Optional additional details
//!   "timestamp": "2024-01-01T12:00:00Z"
//! }
//! ```
//!
//! ## Error Classifications
//!
//! Errors are classified into categories that map to appropriate HTTP status codes:
//!
//! - **Validation Errors** (400 Bad Request): Input validation failures
//! - **Not Found** (404 Not Found): Missing resources
//! - **Upload Errors** (413 Payload Too Large): File upload issues
//! - **Auth Errors** (401 Unauthorized): Authentication failures
//! - **Internal Errors** (500 Internal Server Error): Server-side errors
//!
//! ## Usage
//!
//! ```rust,no_run
//! use crate::error::{AppError, AppResult};
//!
//! fn validate_package_name(name: &str) -> AppResult<()> {
//!     if name.is_empty() {
//!         return Err(AppError::BadRequest("Package name cannot be empty".to_string()));
//!     }
//!     Ok(())
//! }
//! ```

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};

/// Standardized error response structure for consistent API error handling
#[derive(Serialize, Debug)]
pub struct ApiErrorResponse {
    pub error: String,          // Human-readable error message
    pub code: String,           // Machine-readable error code
    pub details: Option<Value>, // Additional error details
    pub timestamp: String,      // ISO 8601 timestamp
}

/// Error code classification for machine-readable error types
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCode {
    ValidationError, // For input validation failures
    NotFound,        // For missing resources
    UploadError,     // For file upload issues
    InternalError,   // For server-side errors
    AuthError,       // For authentication issues
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::ValidationError => "validation_error",
            ErrorCode::NotFound => "not_found",
            ErrorCode::UploadError => "upload_error",
            ErrorCode::InternalError => "internal_error",
            ErrorCode::AuthError => "auth_error",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            ErrorCode::ValidationError => StatusCode::BAD_REQUEST,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::UploadError => StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::AuthError => StatusCode::UNAUTHORIZED,
        }
    }
}

/// Application-specific error types with error codes
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Multipart form parsing error: {0}")]
    Multipart(#[from] axum::extract::multipart::MultipartError),

    #[error("Base64 decoding error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("UTF-8 conversion error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    UploadError(String),

    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl AppError {
    /// Get the appropriate error code for this error type
    pub fn error_code(&self) -> ErrorCode {
        match self {
            AppError::BadRequest(_)
            | AppError::Json(_)
            | AppError::Multipart(_)
            | AppError::Base64(_)
            | AppError::Utf8(_) => ErrorCode::ValidationError,
            AppError::NotFound(_) => ErrorCode::NotFound,
            AppError::UploadError(_) => ErrorCode::UploadError,
            AppError::InternalError(_) => ErrorCode::InternalError,
            AppError::Unauthorized(_) => ErrorCode::AuthError,
            AppError::Io(_) | AppError::Anyhow(_) => ErrorCode::InternalError,
        }
    }

    /// Get additional error details if available
    pub fn details(&self) -> Option<Value> {
        match self {
            AppError::Anyhow(e) => e
                .source()
                .map(|source| json!({"source": source.to_string()})),
            _ => None,
        }
    }

    /// Create a standardized error response
    pub fn to_error_response(&self) -> ApiErrorResponse {
        let code = self.error_code();
        ApiErrorResponse {
            error: self.to_string(),
            code: code.as_str().to_string(),
            details: self.details(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Log the error before moving values out
        tracing::error!(error = %self, "Request failed");

        let error_response = self.to_error_response();
        let status = self.error_code().http_status();

        // Log additional details for internal errors
        if matches!(self.error_code(), ErrorCode::InternalError) {
            if let AppError::Anyhow(ref e) = self {
                tracing::error!(source = ?e.source(), "Internal server error details");
            }
        }

        tracing::debug!(status = %status, code = %error_response.code, "Returning standardized error response");

        (status, axum::Json(error_response)).into_response()
    }
}

/// Convenient result type for application operations.
///
/// This type alias provides a standard Result type using [`AppError`] for all
/// application-level operations, reducing boilerplate in function signatures.
pub type AppResult<T> = Result<T, AppError>;
