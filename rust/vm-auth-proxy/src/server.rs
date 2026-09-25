//! HTTP server for auth proxy service

use crate::storage::SecretStore;
use crate::types::{
    EnvironmentResponse, HealthResponse, SecretListResponse, SecretRequest, SecretResponse,
    SecretScope, SecretSummary,
};
use anyhow::{Context, Result};
use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    middleware,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

/// Shared application state
#[derive(Clone)]
struct AppState {
    store: Arc<Mutex<SecretStore>>,
    start_time: Instant,
}

const MAX_BODY_BYTES: usize = 256 * 1024;

fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/secrets", get(list_secrets))
        .route("/secrets/{name}", post(add_secret))
        .route("/secrets/{name}", get(get_secret))
        .route("/secrets/{name}", delete(remove_secret))
        .route("/env/{vm_name}", get(get_environment))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(
            vm_logging::HttpLogContext::new("auth_proxy"),
            vm_logging::request_context,
        ))
        .with_state(state)
}

fn lock_store<'a>(
    state: &'a AppState,
    operation: &'static str,
) -> Result<MutexGuard<'a, SecretStore>, StatusCode> {
    state.store.lock().map_err(|error| {
        error!(
            component = "auth_proxy",
            operation,
            error = %error,
            "secret store lock failed"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// Query parameters for environment endpoint
#[derive(Debug, Deserialize)]
struct EnvQuery {
    project: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScopeQuery {
    scope: String,
}

fn query_scope(query: &ScopeQuery) -> Result<SecretScope, StatusCode> {
    SecretScope::parse(&query.scope).ok_or(StatusCode::BAD_REQUEST)
}

/// Run the auth proxy server with optional graceful shutdown
pub async fn run_server_with_shutdown(
    host: String,
    port: u16,
    data_dir: PathBuf,
    shutdown_receiver: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<()> {
    let store = SecretStore::new(data_dir).context("Failed to initialize secret store")?;
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
        start_time: Instant::now(),
    };

    let app = app_router(state);

    // Start server
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    info!(
        component = "auth_proxy",
        operation = "listen",
        host,
        port,
        "auth proxy listening"
    );

    match shutdown_receiver {
        Some(shutdown_rx) => {
            // Use graceful shutdown when shutdown receiver is provided
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                    info!(
                        component = "auth_proxy",
                        operation = "shutdown",
                        "auth proxy stopping"
                    );
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
    let store = lock_store(&state, "health_check")?;
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
    Query(query): Query<ScopeQuery>,
    headers: HeaderMap,
) -> Result<Json<SecretListResponse>, StatusCode> {
    // Verify auth token
    if !verify_auth_token(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let scope = query_scope(&query)?;

    let store = lock_store(&state, "list_secrets")?;
    let secrets: Vec<SecretSummary> = store
        .list_secrets()
        .filter(|secret| secret.scope == scope)
        .map(|secret| SecretSummary {
            name: secret.name.clone(),
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

    let mut store = lock_store(&state, "add_secret")?;
    match store.add_secret(&name, &request.value, request.scope, request.description) {
        Ok(()) => {
            let response = SecretResponse {
                name,
                success: true,
                message: Some("Secret added successfully".to_string()),
            };
            Ok(Json(response))
        }
        Err(error) => {
            error!(operation = "add_secret", error = %error, "secret operation failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get a specific secret value
async fn get_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ScopeQuery>,
    headers: HeaderMap,
) -> Result<String, StatusCode> {
    // Verify auth token
    if !verify_auth_token(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let scope = query_scope(&query)?;

    let store = lock_store(&state, "get_secret")?;
    match store.get_secret(&name, &scope) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(error) => {
            error!(operation = "get_secret", error = %error, "secret operation failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Remove a secret
async fn remove_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ScopeQuery>,
    headers: HeaderMap,
) -> Result<Json<SecretResponse>, StatusCode> {
    // Verify auth token
    if !verify_auth_token(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let scope = query_scope(&query)?;

    let mut store = lock_store(&state, "remove_secret")?;
    match store.remove_secret(&name, &scope) {
        Ok(true) => {
            let response = SecretResponse {
                name,
                success: true,
                message: Some("Secret removed successfully".to_string()),
            };
            Ok(Json(response))
        }
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(error) => {
            error!(operation = "remove_secret", error = %error, "secret operation failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
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

    let store = lock_store(&state, "get_environment")?;
    match store.get_env_vars_for_vm(&vm_name, params.project.as_deref()) {
        Ok(env_vars) => {
            let response = EnvironmentResponse {
                env_vars,
                vm_name,
                project_name: params.project,
            };
            Ok(Json(response))
        }
        Err(error) => {
            error!(
                operation = "get_environment",
                error = %error,
                "secret operation failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Verify the authentication token from headers.
///
/// Failed attempts are logged at `warn!` level so operators can spot
/// misconfigured callers and brute-force probing in the audit trail.
fn verify_auth_token(state: &AppState, headers: &HeaderMap) -> bool {
    let store = match state.store.lock() {
        Ok(guard) => guard,
        Err(_) => {
            error!(
                operation = "authenticate",
                error_code = "store_unavailable",
                "authentication failed"
            );
            return false;
        }
    };

    // Get expected token
    let expected_token = match store.get_auth_token() {
        Some(token) => token,
        None => {
            error!(
                operation = "authenticate",
                error_code = "token_unavailable",
                "authentication failed"
            );
            return false;
        }
    };

    // Check Authorization header. Use a constant-time comparison so a remote
    // attacker can't recover the token byte-by-byte via timing side channels.
    if let Some(auth_header) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                use subtle::ConstantTimeEq;
                let ok: bool = token.as_bytes().ct_eq(expected_token.as_bytes()).into();
                if !ok {
                    warn!(
                        operation = "authenticate",
                        error_code = "token_mismatch",
                        "authentication rejected"
                    );
                }
                return ok;
            }
            warn!(
                operation = "authenticate",
                error_code = "invalid_scheme",
                "authentication rejected"
            );
        } else {
            warn!(
                operation = "authenticate",
                error_code = "invalid_header",
                "authentication rejected"
            );
        }
    } else {
        warn!(
            operation = "authenticate",
            error_code = "missing_header",
            "authentication rejected"
        );
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SecretScope;
    use axum_test::TestServer;
    use tempfile::TempDir;

    async fn create_test_server() -> (TestServer, String, TempDir) {
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

        (TestServer::new(app_router(state)), auth_token, temp_dir)
    }

    #[tokio::test]
    async fn test_health_check() {
        let (server, _, _storage) = create_test_server().await;

        let response = server
            .get("/health")
            .add_header(vm_logging::REQUEST_ID_HEADER, "auth-test-1")
            .await;
        response.assert_status_ok();
        assert_eq!(
            response.header(vm_logging::REQUEST_ID_HEADER),
            "auth-test-1"
        );

        let health: HealthResponse = response.json();
        assert_eq!(health.status, "healthy");
        assert_eq!(health.secret_count, 0);
    }

    #[tokio::test]
    async fn test_add_and_get_secret() {
        let (server, token, storage) = create_test_server().await;

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

        let reopened = SecretStore::new(storage.path().to_path_buf()).unwrap();
        assert_eq!(
            reopened
                .get_secret("test_key", &SecretScope::Global)
                .unwrap()
                .as_deref(),
            Some("test-secret-value")
        );

        // Get the secret
        let response = server
            .get("/secrets/test_key?scope=global")
            .add_header("Authorization", format!("Bearer {}", token))
            .await;
        response.assert_status_ok();
        response.assert_text("test-secret-value");
    }

    #[tokio::test]
    async fn test_unauthorized_access() {
        let (server, _, _storage) = create_test_server().await;

        // Try to access without token
        let response = server.get("/secrets?scope=global").await;
        response.assert_status(StatusCode::UNAUTHORIZED);

        // Try with wrong token
        let response = server
            .get("/secrets?scope=global")
            .add_header("Authorization", "Bearer wrong-token")
            .await;
        response.assert_status(StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn failed_secret_write_returns_error_and_preserves_memory() {
        let (server, token, storage) = create_test_server().await;
        let request = SecretRequest {
            value: "value".to_string(),
            scope: SecretScope::Global,
            description: None,
        };
        std::fs::remove_dir_all(storage.path()).unwrap();

        let response = server
            .post("/secrets/key")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&request)
            .await;
        response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);

        let response = server
            .get("/secrets/key?scope=global")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn failed_secret_removal_returns_error_and_preserves_memory() {
        let (server, token, storage) = create_test_server().await;
        let request = SecretRequest {
            value: "value".to_string(),
            scope: SecretScope::Global,
            description: None,
        };
        let response = server
            .post("/secrets/key")
            .add_header("Authorization", format!("Bearer {token}"))
            .json(&request)
            .await;
        response.assert_status_ok();
        std::fs::remove_dir_all(storage.path()).unwrap();

        let response = server
            .delete("/secrets/key?scope=global")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);

        let response = server
            .get("/secrets/key?scope=global")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        response.assert_status_ok();
        response.assert_text("value");
    }

    #[tokio::test]
    async fn test_list_secrets() {
        let (server, token, _storage) = create_test_server().await;

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
            .get("/secrets?scope=global")
            .add_header("Authorization", format!("Bearer {}", token))
            .await;
        response.assert_status_ok();

        let list: SecretListResponse = response.json();
        assert_eq!(list.total, 1);
        assert_eq!(list.secrets.len(), 1);
    }

    #[tokio::test]
    async fn scoped_http_operations_do_not_cross_secret_namespaces() {
        let (server, token, _storage) = create_test_server().await;
        for (scope, value) in [
            (SecretScope::Global, "user-value"),
            (SecretScope::Project("demo".into()), "project-value"),
        ] {
            server
                .post("/secrets/TOKEN")
                .add_header("Authorization", format!("Bearer {token}"))
                .json(&SecretRequest {
                    value: value.into(),
                    scope,
                    description: None,
                })
                .await
                .assert_status_ok();
        }
        let user = server
            .get("/secrets/TOKEN?scope=global")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        user.assert_text("user-value");
        let project = server
            .get("/secrets/TOKEN?scope=project%3Ademo")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        project.assert_text("project-value");

        let project_list = server
            .get("/secrets?scope=project%3Ademo")
            .add_header("Authorization", format!("Bearer {token}"))
            .await;
        assert!(!project_list.text().contains("project-value"));
        let list: SecretListResponse = project_list.json();
        assert_eq!(list.total, 1);
        assert_eq!(list.secrets[0].name, "TOKEN");

        server
            .delete("/secrets/TOKEN?scope=project%3Ademo")
            .add_header("Authorization", format!("Bearer {token}"))
            .await
            .assert_status_ok();
        server
            .get("/secrets/TOKEN?scope=project%3Ademo")
            .add_header("Authorization", format!("Bearer {token}"))
            .await
            .assert_status(StatusCode::NOT_FOUND);
        server
            .get("/secrets/TOKEN?scope=global")
            .add_header("Authorization", format!("Bearer {token}"))
            .await
            .assert_text("user-value");
    }
}
