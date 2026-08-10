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

fn extract_token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(vm_packages::authorization_token)
}

pub fn validate_read_headers(config: &Config, headers: &HeaderMap) -> Result<(), AppError> {
    validate_headers(config, headers, true)
}

pub fn validate_publish_headers(config: &Config, headers: &HeaderMap) -> Result<(), AppError> {
    validate_headers(config, headers, false)
}

fn validate_headers(
    config: &Config,
    headers: &HeaderMap,
    allow_read_key: bool,
) -> Result<(), AppError> {
    if !config.security.require_authentication {
        return Ok(());
    }

    let token = extract_token_from_headers(headers).ok_or_else(|| {
        AppError::Unauthorized("Missing or invalid Authorization header".to_string())
    })?;

    let is_publisher =
        config.security.publish_keys.contains(&token) || config.security.api_keys.contains(&token);
    let is_reader = allow_read_key && config.security.read_keys.contains(&token);
    if !is_publisher && !is_reader {
        return Err(AppError::Unauthorized("Invalid API key".to_string()));
    }

    Ok(())
}

pub async fn read_auth_middleware(
    State(config): State<Arc<Config>>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Skip auth if not required
    if !config.security.require_authentication {
        return Ok(next.run(req).await);
    }

    validate_read_headers(&config, req.headers())?;

    // Token is valid, proceed with request
    Ok(next.run(req).await)
}

/// Check if authentication is required based on config
pub fn is_auth_required(config: &Config) -> bool {
    config.security.require_authentication
        && (!config.security.read_keys.is_empty()
            || !config.security.publish_keys.is_empty()
            || !config.security.api_keys.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_config() -> Config {
        let mut config = Config::default();
        config.security.require_authentication = true;
        config.security.read_keys = vec!["read-token".to_string()];
        config.security.publish_keys = vec!["publish-token".to_string()];
        config
    }

    #[test]
    fn auth_disabled_allows_missing_header() {
        let config = Config::default();
        let headers = HeaderMap::new();

        assert!(validate_read_headers(&config, &headers).is_ok());
    }

    #[test]
    fn auth_enabled_requires_valid_bearer_token() {
        let config = auth_config();
        let mut headers = HeaderMap::new();

        assert!(validate_read_headers(&config, &headers).is_err());

        headers.insert(
            header::AUTHORIZATION,
            "Bearer wrong-token".parse().expect("valid header"),
        );
        assert!(validate_read_headers(&config, &headers).is_err());

        headers.insert(
            header::AUTHORIZATION,
            "Bearer read-token".parse().expect("valid header"),
        );
        assert!(validate_read_headers(&config, &headers).is_ok());
        assert!(validate_publish_headers(&config, &headers).is_err());
    }

    #[test]
    fn basic_and_raw_tokens_support_package_manager_protocols() {
        let config = auth_config();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            "Basic cmVhZGVyOnJlYWQtdG9rZW4="
                .parse()
                .expect("valid header"),
        );
        assert!(validate_read_headers(&config, &headers).is_ok());

        headers.insert(
            header::AUTHORIZATION,
            "publish-token".parse().expect("valid header"),
        );
        assert!(validate_publish_headers(&config, &headers).is_ok());
        assert!(validate_read_headers(&config, &headers).is_ok());
    }
}
