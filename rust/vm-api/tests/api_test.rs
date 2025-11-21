//! Integration tests for REST API endpoints
//!
//! Tests the HTTP API endpoints including workspace creation, listing,
//! retrieval, and deletion.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use vm_orchestrator::{CreateWorkspaceRequest, Workspace};

#[tokio::test]
async fn test_create_workspace_endpoint() {
    let (client, _) = common::TestClient::new_with_db().await;

    let req_body = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "will-be-overridden".to_string(), // Should be overridden by auth
        repo_url: Some("https://github.com/test/repo".to_string()),
        template: Some("nodejs".to_string()),
        provider: Some("docker".to_string()),
        ttl_seconds: Some(3600),
    };

    let response = client
        .post(
            "/api/v1/workspaces",
            &req_body,
            Some(common::auth_headers("testuser")),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let workspace: Workspace = common::extract_json_body(response).await;

    assert_eq!(workspace.name, "test-workspace");
    assert_eq!(workspace.owner, "testuser"); // Should be from auth header
    assert_eq!(workspace.template, Some("nodejs".to_string()));
}

#[tokio::test]
async fn test_create_workspace_without_auth_fails() {
    let (client, _) = common::TestClient::new_with_db().await;

    let req_body = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    let response = client.post("/api/v1/workspaces", &req_body, None).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_list_workspaces_endpoint() {
    let pool = common::create_test_db().await;

    // Create some test workspaces
    common::fixture_workspaces_for_user(&pool, "alice", 3).await;
    common::fixture_workspaces_for_user(&pool, "bob", 2).await;

    let client = common::TestClient::new(pool).await;

    // List workspaces for alice
    let response = client
        .get("/api/v1/workspaces", Some(common::auth_headers("alice")))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let workspaces: Vec<Workspace> = common::extract_json_body(response).await;

    assert_eq!(workspaces.len(), 3);
    assert!(workspaces.iter().all(|w| w.owner == "alice"));
}

#[tokio::test]
async fn test_list_workspaces_filters_by_owner() {
    let pool = common::create_test_db().await;

    // Create workspaces for different users
    common::fixture_workspaces_for_user(&pool, "alice", 3).await;
    common::fixture_workspaces_for_user(&pool, "bob", 2).await;

    let client = common::TestClient::new(pool).await;

    // List workspaces for bob
    let response = client
        .get("/api/v1/workspaces", Some(common::auth_headers("bob")))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let workspaces: Vec<Workspace> = common::extract_json_body(response).await;

    // Should only see bob's workspaces
    assert_eq!(workspaces.len(), 2);
    assert!(workspaces.iter().all(|w| w.owner == "bob"));
}

#[tokio::test]
async fn test_get_workspace_endpoint() {
    let pool = common::create_test_db().await;
    let workspace = common::fixture_workspace(&pool, "test-workspace", "testuser").await;

    let client = common::TestClient::new(pool).await;

    let response = client
        .get(
            &format!("/api/v1/workspaces/{}", workspace.id),
            Some(common::auth_headers("testuser")),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let fetched_workspace: Workspace = common::extract_json_body(response).await;

    assert_eq!(fetched_workspace.id, workspace.id);
    assert_eq!(fetched_workspace.name, "test-workspace");
}

#[tokio::test]
async fn test_get_nonexistent_workspace_returns_404() {
    let (client, _) = common::TestClient::new_with_db().await;

    let response = client
        .get(
            "/api/v1/workspaces/nonexistent-id",
            Some(common::auth_headers("testuser")),
        )
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_workspace_endpoint() {
    let pool = common::create_test_db().await;
    let workspace = common::fixture_workspace(&pool, "test-workspace", "testuser").await;

    let client = common::TestClient::new(pool.clone()).await;

    let response = client
        .delete(
            &format!("/api/v1/workspaces/{}", workspace.id),
            Some(common::auth_headers("testuser")),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let json: serde_json::Value = common::extract_json_body(response).await;
    assert_eq!(json["message"], "Workspace deleted");

    // Verify workspace is actually deleted
    let orchestrator = vm_orchestrator::WorkspaceOrchestrator::new(pool);
    let result = orchestrator.get_workspace(&workspace.id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_delete_nonexistent_workspace_returns_404() {
    let (client, _) = common::TestClient::new_with_db().await;

    let response = client
        .delete(
            "/api/v1/workspaces/nonexistent-id",
            Some(common::auth_headers("testuser")),
        )
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_workspace_with_all_fields() {
    let (client, _) = common::TestClient::new_with_db().await;

    let req_body = CreateWorkspaceRequest {
        name: "full-workspace".to_string(),
        owner: "ignored".to_string(),
        repo_url: Some("https://github.com/org/repo".to_string()),
        template: Some("python".to_string()),
        provider: Some("docker".to_string()),
        ttl_seconds: Some(7200),
    };

    let response = client
        .post(
            "/api/v1/workspaces",
            &req_body,
            Some(common::auth_headers("alice")),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let workspace: Workspace = common::extract_json_body(response).await;

    assert_eq!(workspace.name, "full-workspace");
    assert_eq!(workspace.owner, "alice");
    assert_eq!(
        workspace.repo_url,
        Some("https://github.com/org/repo".to_string())
    );
    assert_eq!(workspace.template, Some("python".to_string()));
    assert_eq!(workspace.provider, "docker");
    assert_eq!(workspace.ttl_seconds, Some(7200));
    assert!(workspace.expires_at.is_some());
}

#[tokio::test]
async fn test_create_workspace_minimal_fields() {
    let (client, _) = common::TestClient::new_with_db().await;

    let req_body = CreateWorkspaceRequest {
        name: "minimal".to_string(),
        owner: "ignored".to_string(),
        repo_url: None,
        template: None,
        provider: None, // Should default to docker
        ttl_seconds: None,
    };

    let response = client
        .post(
            "/api/v1/workspaces",
            &req_body,
            Some(common::auth_headers("bob")),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let workspace: Workspace = common::extract_json_body(response).await;

    assert_eq!(workspace.name, "minimal");
    assert_eq!(workspace.owner, "bob");
    assert_eq!(workspace.provider, "docker"); // Default provider
    assert!(workspace.repo_url.is_none());
    assert!(workspace.template.is_none());
    assert!(workspace.ttl_seconds.is_none());
    assert!(workspace.expires_at.is_none());
}

#[tokio::test]
async fn test_list_empty_workspaces() {
    let (client, _) = common::TestClient::new_with_db().await;

    let response = client
        .get("/api/v1/workspaces", Some(common::auth_headers("newuser")))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let workspaces: Vec<Workspace> = common::extract_json_body(response).await;

    assert_eq!(workspaces.len(), 0);
}

#[tokio::test]
async fn test_create_multiple_workspaces_for_same_user() {
    let (client, _) = common::TestClient::new_with_db().await;

    // Create first workspace
    let req_body1 = CreateWorkspaceRequest {
        name: "workspace-1".to_string(),
        owner: "ignored".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    let response1 = client
        .post(
            "/api/v1/workspaces",
            &req_body1,
            Some(common::auth_headers("alice")),
        )
        .await;
    assert_eq!(response1.status(), StatusCode::OK);

    // Create second workspace
    let req_body2 = CreateWorkspaceRequest {
        name: "workspace-2".to_string(),
        owner: "ignored".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    let response2 = client
        .post(
            "/api/v1/workspaces",
            &req_body2,
            Some(common::auth_headers("alice")),
        )
        .await;
    assert_eq!(response2.status(), StatusCode::OK);

    // List workspaces - should see both
    let response3 = client
        .get("/api/v1/workspaces", Some(common::auth_headers("alice")))
        .await;
    assert_eq!(response3.status(), StatusCode::OK);

    let workspaces: Vec<Workspace> = common::extract_json_body(response3).await;
    assert_eq!(workspaces.len(), 2);
}

#[tokio::test]
async fn test_workspace_json_serialization() {
    let pool = common::create_test_db().await;
    let workspace = common::fixture_workspace_with_ttl(&pool, "test", "testuser", 3600).await;

    let client = common::TestClient::new(pool).await;

    let response = client
        .get(
            &format!("/api/v1/workspaces/{}", workspace.id),
            Some(common::auth_headers("testuser")),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Verify JSON structure
    assert!(json["id"].is_string());
    assert_eq!(json["name"], "test");
    assert_eq!(json["owner"], "testuser");
    assert_eq!(json["provider"], "docker");
    assert!(json["created_at"].is_string());
    assert!(json["updated_at"].is_string());
    assert!(json["expires_at"].is_string());
    assert_eq!(json["ttl_seconds"], 3600);
}

#[tokio::test]
async fn test_invalid_json_returns_400() {
    let (client, _) = common::TestClient::new_with_db().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/workspaces")
        .header("content-type", "application/json")
        .header("x-user", "testuser")
        .body(Body::from("invalid json"))
        .unwrap();

    let response = client.send_request(request).await;

    // Axum returns 422 for invalid JSON
    assert!(
        response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn test_health_endpoint() {
    let (client, _) = common::TestClient::new_with_db().await;

    let response = client.get("/health", None).await;

    assert_eq!(response.status(), StatusCode::OK);

    let json: serde_json::Value = common::extract_json_body(response).await;
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "vm-api");
}

#[tokio::test]
async fn test_readiness_endpoint() {
    let (client, _) = common::TestClient::new_with_db().await;

    let response = client.get("/health/ready", None).await;

    assert_eq!(response.status(), StatusCode::OK);

    let json: serde_json::Value = common::extract_json_body(response).await;
    assert_eq!(json["status"], "ready");
    assert_eq!(json["database"], "connected");
}
