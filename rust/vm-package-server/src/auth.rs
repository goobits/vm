//! Simple bearer token authentication middleware for package uploads
//!
//! This module provides optional authentication for upload/publish endpoints.
//! When enabled via config, it validates Bearer tokens from the Authorization header.

use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::{config::Config, error::AppError};

/// Extract bearer token from Authorization header
fn extract_bearer_token(req: &Request) -> Option<String> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer ").map(|token| token.to_string()))
}

/// Middleware to validate authentication for upload endpoints
pub async fn auth_middleware(
    State(config): State<Arc<Config>>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Skip auth if not required
    if !config.security.require_authentication {
        return Ok(next.run(req).await);
    }

    // Extract token from request
    let token = extract_bearer_token(&req).ok_or_else(|| {
        AppError::Unauthorized("Missing or invalid Authorization header".to_string())
    })?;

    // Validate token against configured API keys
    if !config.security.api_keys.contains(&token) {
        return Err(AppError::Unauthorized("Invalid API key".to_string()));
    }

    // Token is valid, proceed with request
    Ok(next.run(req).await)
}

/// Check if authentication is required based on config
pub fn is_auth_required(config: &Config) -> bool {
    config.security.require_authentication && !config.security.api_keys.is_empty()
}
