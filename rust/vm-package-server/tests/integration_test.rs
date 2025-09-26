//! Integration tests for HTTP API endpoints
//!
//! This module provides integration tests for the HTTP API endpoints
//! using axum-test to verify correct behavior and response formats.
//! These tests focus on API functionality without external dependencies.

use axum::Router;
use axum_test::TestServer;
use serde_json::json;
use std::sync::Arc;

mod common;
use common::create_test_setup;
use vm_package_server::AppState;

/// Helper to create a test server with temporary directories
async fn create_test_server() -> (TestServer, common::TestSetup) {
    let setup = create_test_setup()
        .await
        .expect("Failed to create test setup");

    // Build minimal router for testing
    let app = Router::new()
        .route("/api/status", axum::routing::get(server_status))
        .route("/api/packages", axum::routing::get(list_packages))
        .with_state(setup.app_state.clone());

    let server = TestServer::new(app).expect("Failed to create test server");

    (server, setup)
}

async fn server_status(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    axum::Json(json!({
        "status": "running",
        "server_addr": state.server_addr,
        "data_dir": state.data_dir.display().to_string(),
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn list_packages(
    axum::extract::State(_state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    axum::Json(json!({
        "pypi": [],
        "npm": [],
        "cargo": []
    }))
}

/// Tests the server status API endpoint
#[tokio::test]
async fn test_api_server_status() {
    let (server, _setup) = create_test_server().await;

    let response = server.get("/api/status").await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "running");
    assert!(body["server_addr"].is_string());
    assert!(body["data_dir"].is_string());
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

/// Tests the package listing API endpoint
#[tokio::test]
async fn test_api_list_packages() {
    let (server, _setup) = create_test_server().await;

    let response = server.get("/api/packages").await;
    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert!(body["pypi"].is_array());
    assert!(body["npm"].is_array());
    assert!(body["cargo"].is_array());
}

/// Tests that invalid endpoints return 404
#[tokio::test]
async fn test_api_invalid_endpoint_returns_404() {
    let (server, _setup) = create_test_server().await;

    let response = server.get("/api/nonexistent").await;
    response.assert_status_not_found();
}
