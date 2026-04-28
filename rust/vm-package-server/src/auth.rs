//! Simple bearer token authentication middleware for package uploads
//!
//! This module provides optional authentication for upload/publish endpoints.
//! When enabled via config, it validates Bearer tokens from the Authorization header.

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::{config::Config, error::AppError};

fn extract_bearer_token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer ").map(|token| token.to_string()))
}

pub fn validate_auth_headers(config: &Config, headers: &HeaderMap) -> Result<(), AppError> {
    if !config.security.require_authentication {
        return Ok(());
    }

    let token = extract_bearer_token_from_headers(headers).ok_or_else(|| {
        AppError::Unauthorized("Missing or invalid Authorization header".to_string())
    })?;

    if !config.security.api_keys.contains(&token) {
        return Err(AppError::Unauthorized("Invalid API key".to_string()));
    }

    Ok(())
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

    validate_auth_headers(&config, req.headers())?;

    // Token is valid, proceed with request
    Ok(next.run(req).await)
}

/// Check if authentication is required based on config
pub fn is_auth_required(config: &Config) -> bool {
    config.security.require_authentication && !config.security.api_keys.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_config() -> Config {
        let mut config = Config::default();
        config.security.require_authentication = true;
        config.security.api_keys = vec!["secret-token".to_string()];
        config
    }

    #[test]
    fn auth_disabled_allows_missing_header() {
        let config = Config::default();
        let headers = HeaderMap::new();

        assert!(validate_auth_headers(&config, &headers).is_ok());
    }

    #[test]
    fn auth_enabled_requires_valid_bearer_token() {
        let config = auth_config();
        let mut headers = HeaderMap::new();

        assert!(validate_auth_headers(&config, &headers).is_err());

        headers.insert(
            header::AUTHORIZATION,
            "Bearer wrong-token".parse().expect("valid header"),
        );
        assert!(validate_auth_headers(&config, &headers).is_err());

        headers.insert(
            header::AUTHORIZATION,
            "Bearer secret-token".parse().expect("valid header"),
        );
        assert!(validate_auth_headers(&config, &headers).is_ok());
    }
}
