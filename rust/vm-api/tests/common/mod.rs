//! Common test utilities and helpers for vm-api tests
//!
//! This module provides shared functionality for all test files to reduce code duplication
//! and improve maintainability of the test suite.

#![allow(dead_code)]

use axum::Router;
use sqlx::SqlitePool;
use vm_orchestrator::{CreateWorkspaceRequest, Workspace};

/// Helper to create an in-memory test database with migrations
pub async fn create_test_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");

    // Run migrations from vm-orchestrator
    sqlx::migrate!("../vm-orchestrator/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}

/// Create a test app with the given database pool
pub async fn create_test_app(pool: SqlitePool) -> Router {
    vm_api::create_app(pool)
        .await
        .expect("Failed to create test app")
}

/// Create a test workspace request with default values
pub fn create_test_workspace_request(name: &str, owner: &str) -> CreateWorkspaceRequest {
    CreateWorkspaceRequest {
        name: name.to_string(),
        owner: owner.to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    }
}

/// Create a test workspace request with all fields
pub fn create_full_workspace_request(
    name: &str,
    owner: &str,
    repo_url: Option<&str>,
    template: Option<&str>,
    provider: Option<&str>,
    ttl_seconds: Option<i64>,
) -> CreateWorkspaceRequest {
    CreateWorkspaceRequest {
        name: name.to_string(),
        owner: owner.to_string(),
        repo_url: repo_url.map(|s| s.to_string()),
        template: template.map(|s| s.to_string()),
        provider: provider.map(|s| s.to_string()),
        ttl_seconds,
    }
}

/// Fixture: Create a simple workspace for testing
pub async fn fixture_workspace(pool: &SqlitePool, name: &str, owner: &str) -> Workspace {
    let orchestrator = vm_orchestrator::WorkspaceOrchestrator::new(pool.clone());
    let req = create_test_workspace_request(name, owner);

    orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create fixture workspace")
}

/// Fixture: Create a workspace with TTL
pub async fn fixture_workspace_with_ttl(
    pool: &SqlitePool,
    name: &str,
    owner: &str,
    ttl_seconds: i64,
) -> Workspace {
    let orchestrator = vm_orchestrator::WorkspaceOrchestrator::new(pool.clone());
    let req = CreateWorkspaceRequest {
        name: name.to_string(),
        owner: owner.to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(ttl_seconds),
    };

    orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create fixture workspace with TTL")
}

/// Fixture: Create an expired workspace
pub async fn fixture_expired_workspace(pool: &SqlitePool, name: &str, owner: &str) -> Workspace {
    fixture_workspace_with_ttl(pool, name, owner, -3600).await
}

/// Fixture: Create multiple workspaces for the same user
pub async fn fixture_workspaces_for_user(
    pool: &SqlitePool,
    owner: &str,
    count: usize,
) -> Vec<Workspace> {
    let mut workspaces = Vec::new();

    for i in 0..count {
        let workspace = fixture_workspace(pool, &format!("{}-workspace-{}", owner, i), owner).await;
        workspaces.push(workspace);
    }

    workspaces
}

/// Helper to extract JSON body from axum response
pub async fn extract_json_body<T>(response: axum::response::Response) -> T
where
    T: serde::de::DeserializeOwned,
{
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("Failed to read response body");

    serde_json::from_slice(&body).expect("Failed to deserialize JSON")
}

/// Helper to create authenticated request headers
pub fn auth_headers(username: &str) -> Vec<(&'static str, &str)> {
    vec![("x-user", username)]
}

/// Helper to create authenticated request headers with email
pub fn auth_headers_with_email<'a>(username: &'a str, email: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![("x-user", username), ("x-vm-email", email)]
}

/// TestClient to encapsulate API interaction logic
pub struct TestClient {
    pub app: Router,
}

impl TestClient {
    /// Create a new TestClient
    pub async fn new(pool: SqlitePool) -> Self {
        let app = create_test_app(pool).await;
        Self { app }
    }

    /// Create a new TestClient with a new in-memory DB
    pub async fn new_with_db() -> (Self, SqlitePool) {
        let pool = create_test_db().await;
        let client = Self::new(pool.clone()).await;
        (client, pool)
    }

    /// Send a request to the API
    pub async fn send_request(
        &self,
        request: axum::http::Request<axum::body::Body>,
    ) -> axum::http::Response<axum::body::Body> {
        // Clone the app to allow reuse (Router is cheap to clone)
        use tower::ServiceExt;
        self.app.clone().oneshot(request).await.unwrap()
    }

    /// Post JSON to an endpoint
    pub async fn post<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
        headers: Option<Vec<(&str, &str)>>,
    ) -> axum::http::Response<axum::body::Body> {
        let req_body = serde_json::to_string(body).expect("Failed to serialize request body");
        let mut builder = axum::http::Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");

        if let Some(h) = headers {
            for (k, v) in h {
                builder = builder.header(k, v);
            }
        }

        let request = builder.body(axum::body::Body::from(req_body)).unwrap();
        self.send_request(request).await
    }

    /// Get request to an endpoint
    pub async fn get(
        &self,
        uri: &str,
        headers: Option<Vec<(&str, &str)>>,
    ) -> axum::http::Response<axum::body::Body> {
        let mut builder = axum::http::Request::builder().method("GET").uri(uri);

        if let Some(h) = headers {
            for (k, v) in h {
                builder = builder.header(k, v);
            }
        }

        let request = builder.body(axum::body::Body::empty()).unwrap();
        self.send_request(request).await
    }

    /// Delete request to an endpoint
    pub async fn delete(
        &self,
        uri: &str,
        headers: Option<Vec<(&str, &str)>>,
    ) -> axum::http::Response<axum::body::Body> {
        let mut builder = axum::http::Request::builder().method("DELETE").uri(uri);

        if let Some(h) = headers {
            for (k, v) in h {
                builder = builder.header(k, v);
            }
        }

        let request = builder.body(axum::body::Body::empty()).unwrap();
        self.send_request(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_test_db() {
        let pool = create_test_db().await;

        // Verify tables exist
        let result: Result<(i64,), _> = sqlx::query_as("SELECT COUNT(*) FROM workspaces")
            .fetch_one(&pool)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_fixture_workspace() {
        let pool = create_test_db().await;
        let workspace = fixture_workspace(&pool, "test", "testuser").await;

        assert_eq!(workspace.name, "test");
        assert_eq!(workspace.owner, "testuser");
    }

    #[tokio::test]
    async fn test_fixture_expired_workspace() {
        let pool = create_test_db().await;
        let workspace = fixture_expired_workspace(&pool, "expired", "testuser").await;

        assert!(workspace.expires_at.is_some());
        assert!(workspace.ttl_seconds.is_some());

        // Verify it's actually expired
        let orchestrator = vm_orchestrator::WorkspaceOrchestrator::new(pool);
        let expired = orchestrator
            .get_expired_workspaces()
            .await
            .expect("Failed to get expired workspaces");

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id, workspace.id);
    }

    #[tokio::test]
    async fn test_fixture_workspaces_for_user() {
        let pool = create_test_db().await;
        let workspaces = fixture_workspaces_for_user(&pool, "alice", 3).await;

        assert_eq!(workspaces.len(), 3);
        assert!(workspaces.iter().all(|w| w.owner == "alice"));
    }
}
