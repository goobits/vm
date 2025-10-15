//! HTTP server for auth proxy service

use crate::storage::{get_auth_data_dir, SecretStore};
use crate::types::{
    EnvironmentResponse, HealthResponse, SecretListResponse, SecretRequest, SecretResponse,
    SecretSummary,
};
use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

/// Shared application state
#[derive(Clone)]
struct AppState {
    store: Arc<Mutex<SecretStore>>,
    start_time: Instant,
}

/// Query parameters for environment endpoint
#[derive(Debug, Deserialize)]
struct EnvQuery {
    project: Option<String>,
}

/// Run the auth proxy server with optional graceful shutdown
pub async fn run_server_with_shutdown(
    host: String,
    port: u16,
    data_dir: PathBuf,
    shutdown_receiver: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<()> {
    info!("Starting auth proxy server on {}:{}", host, port);

    // Initialize secret store
    let store = SecretStore::new(data_dir).context("Failed to initialize secret store")?;
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
        start_time: Instant::now(),
    };

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/secrets", get(list_secrets))
        .route("/secrets/{name}", post(add_secret))
        .route("/secrets/{name}", get(get_secret))
        .route("/secrets/{name}", delete(remove_secret))
        .route("/env/{vm_name}", get(get_environment))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    info!("Auth proxy server listening on {}", addr);

    match shutdown_receiver {
        Some(shutdown_rx) => {
            // Use graceful shutdown when shutdown receiver is provided
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                    info!("Received shutdown signal, stopping auth proxy gracefully");
                })
                .await
                .context("Server failed to start")?;
        }
        None => {
            // Original behavior - run indefinitely
            axum::serve(listener, app)
                .await
                .context("Server failed to start")?;
        }
    }

    Ok(())
}

/// Run the auth proxy server in foreground
pub async fn run_server(host: String, port: u16, data_dir: PathBuf) -> Result<()> {
    run_server_with_shutdown(host, port, data_dir, None).await
}

pub async fn run_server_background(host: String, port: u16, data_dir: PathBuf) -> Result<()> {
    run_server_with_shutdown(host, port, data_dir, None).await
}

/// Check if the auth proxy server is running
pub async fn check_server_running(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    match reqwest::get(&url).await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

/// Health check endpoint
async fn health_check(State(state): State<AppState>) -> Result<Json<HealthResponse>, StatusCode> {
    let store = state.store.lock().map_err(|e| {
        error!("Mutex lock failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let response = HealthResponse {
        status: "healthy".to_string(),
        secret_count: store.secret_count(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.start_time.elapsed().as_secs(),
    };
    Ok(Json(response))
}

/// List all secrets (metadata only)
async fn list_secrets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SecretListResponse>, StatusCode> {
    // Verify auth token
    if !verify_auth_token(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let store = state.store.lock().map_err(|e| {
        error!("Mutex lock failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let secrets: Vec<SecretSummary> = store
        .list_secrets()
        .iter()
        .map(|(name, secret)| SecretSummary {
            name: name.clone(),
            created_at: secret.created_at,
            updated_at: secret.updated_at,
            scope: secret.scope.clone(),
            description: secret.description.clone(),
        })
        .collect();

    let response = SecretListResponse {
        total: secrets.len(),
        secrets,
    };

    Ok(Json(response))
}

/// Add or update a secret
async fn add_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(request): Json<SecretRequest>,
) -> Result<Json<SecretResponse>, StatusCode> {
    // Verify auth token
    if !verify_auth_token(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut store = state.store.lock().map_err(|e| {
        error!("Mutex lock failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match store.add_secret(&name, &request.value, request.scope, request.description) {
        Ok(()) => {
            let response = SecretResponse {
                name,
                success: true,
                message: Some("Secret added successfully".to_string()),
            };
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Failed to add secret: {}", e);
            let response = SecretResponse {
                name,
                success: false,
                message: Some(format!("Failed to add secret: {e}")),
            };
            Ok(Json(response))
        }
    }
}

/// Get a specific secret value
async fn get_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<String, StatusCode> {
    // Verify auth token
    if !verify_auth_token(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let store = state.store.lock().map_err(|e| {
        error!("Mutex lock failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match store.get_secret(&name) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            warn!("Failed to get secret: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Remove a secret
async fn remove_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Json<SecretResponse>, StatusCode> {
    // Verify auth token
    if !verify_auth_token(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut store = state.store.lock().map_err(|e| {
        error!("Mutex lock failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match store.remove_secret(&name) {
        Ok(true) => {
            let response = SecretResponse {
                name,
                success: true,
                message: Some("Secret removed successfully".to_string()),
            };
            Ok(Json(response))
        }
        Ok(false) => {
            let response = SecretResponse {
                name,
                success: false,
                message: Some("Secret not found".to_string()),
            };
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Failed to remove secret: {}", e);
            let response = SecretResponse {
                name,
                success: false,
                message: Some(format!("Failed to remove secret: {e}")),
            };
            Ok(Json(response))
        }
    }
}

/// Get environment variables for a VM
async fn get_environment(
    State(state): State<AppState>,
    Path(vm_name): Path<String>,
    Query(params): Query<EnvQuery>,
    headers: HeaderMap,
) -> Result<Json<EnvironmentResponse>, StatusCode> {
    // Verify auth token
    if !verify_auth_token(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let store = state.store.lock().map_err(|e| {
        error!("Mutex lock failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match store.get_env_vars_for_vm(&vm_name, params.project.as_deref()) {
        Ok(env_vars) => {
            let response = EnvironmentResponse {
                env_vars,
                vm_name,
                project_name: params.project,
            };
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Failed to get environment variables: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Verify the authentication token from headers
fn verify_auth_token(state: &AppState, headers: &HeaderMap) -> bool {
    let store = match state.store.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };

    // Get expected token
    let expected_token = match store.get_auth_token() {
        Some(token) => token,
        None => return false,
    };

    // Check Authorization header
    if let Some(auth_header) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return token == expected_token;
            }
        }
    }

    false
}

/// Use default auth data directory for background server
pub async fn run_server_with_defaults() -> Result<()> {
    let data_dir = get_auth_data_dir()?;
    run_server("127.0.0.1".to_string(), 3090, data_dir).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SecretScope;
    use axum_test::TestServer;
    use tempfile::TempDir;

    async fn create_test_server() -> (TestServer, String) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let store =
            SecretStore::new(temp_dir.path().to_path_buf()).expect("Failed to create SecretStore");
        let auth_token = store
            .get_auth_token()
            .expect("Failed to get auth token")
            .to_string();

        let state = AppState {
            store: Arc::new(Mutex::new(store)),
            start_time: Instant::now(),
        };

        let app = Router::new()
            .route("/health", get(health_check))
            .route("/secrets", get(list_secrets))
            .route("/secrets/{name}", post(add_secret))
            .route("/secrets/{name}", get(get_secret))
            .route("/secrets/{name}", delete(remove_secret))
            .route("/env/{vm_name}", get(get_environment))
            .with_state(state);

        (
            TestServer::new(app).expect("Failed to create test server"),
            auth_token,
        )
    }

    #[tokio::test]
    async fn test_health_check() {
        let (server, _) = create_test_server().await;

        let response = server.get("/health").await;
        response.assert_status_ok();

        let health: HealthResponse = response.json();
        assert_eq!(health.status, "healthy");
        assert_eq!(health.secret_count, 0);
    }

    #[tokio::test]
    async fn test_add_and_get_secret() {
        let (server, token) = create_test_server().await;

        // Add a secret
        let request = SecretRequest {
            value: "test-secret-value".to_string(),
            scope: SecretScope::Global,
            description: Some("Test secret".to_string()),
        };

        let response = server
            .post("/secrets/test_key")
            .add_header("Authorization", format!("Bearer {}", token))
            .json(&request)
            .await;
        response.assert_status_ok();

        // Get the secret
        let response = server
            .get("/secrets/test_key")
            .add_header("Authorization", format!("Bearer {}", token))
            .await;
        response.assert_status_ok();
        response.assert_text("test-secret-value");
    }

    #[tokio::test]
    async fn test_unauthorized_access() {
        let (server, _) = create_test_server().await;

        // Try to access without token
        let response = server.get("/secrets").await;
        response.assert_status(StatusCode::UNAUTHORIZED);

        // Try with wrong token
        let response = server
            .get("/secrets")
            .add_header("Authorization", "Bearer wrong-token")
            .await;
        response.assert_status(StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_list_secrets() {
        let (server, token) = create_test_server().await;

        // Add some secrets first
        let request = SecretRequest {
            value: "value1".to_string(),
            scope: SecretScope::Global,
            description: None,
        };
        server
            .post("/secrets/key1")
            .add_header("Authorization", format!("Bearer {}", token))
            .json(&request)
            .await;

        let request = SecretRequest {
            value: "value2".to_string(),
            scope: SecretScope::Project("test".to_string()),
            description: Some("Project secret".to_string()),
        };
        server
            .post("/secrets/key2")
            .add_header("Authorization", format!("Bearer {}", token))
            .json(&request)
            .await;

        // List secrets
        let response = server
            .get("/secrets")
            .add_header("Authorization", format!("Bearer {}", token))
            .await;
        response.assert_status_ok();

        let list: SecretListResponse = response.json();
        assert_eq!(list.total, 2);
        assert_eq!(list.secrets.len(), 2);
    }
}
