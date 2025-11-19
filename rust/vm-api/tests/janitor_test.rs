//! Integration tests for the janitor cleanup task
//!
//! Tests that the janitor correctly identifies and deletes expired workspaces
//! based on TTL calculations.

use chrono::Utc;
use sqlx::SqlitePool;
use vm_orchestrator::{CreateWorkspaceRequest, WorkspaceOrchestrator};

/// Helper to create an in-memory test database with migrations
async fn create_test_db() -> SqlitePool {
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

#[tokio::test]
async fn test_janitor_finds_expired_workspaces() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create an expired workspace (negative TTL = already expired)
    let req = CreateWorkspaceRequest {
        name: "expired-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(-3600), // Expired 1 hour ago
    };

    orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Get expired workspaces (what janitor does)
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");

    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].name, "expired-workspace");
}

#[tokio::test]
async fn test_janitor_ignores_non_expired_workspaces() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create a valid workspace that expires in the future
    let req = CreateWorkspaceRequest {
        name: "valid-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(7200), // Expires in 2 hours
    };

    orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Get expired workspaces
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");

    // Should be empty
    assert_eq!(expired.len(), 0);
}

#[tokio::test]
async fn test_janitor_ignores_workspaces_without_ttl() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create a workspace without TTL
    let req = CreateWorkspaceRequest {
        name: "no-ttl-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None, // No expiration
    };

    orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Get expired workspaces
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");

    // Should be empty (no TTL = never expires)
    assert_eq!(expired.len(), 0);
}

#[tokio::test]
async fn test_janitor_deletes_expired_workspace() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create an expired workspace
    let req = CreateWorkspaceRequest {
        name: "expired-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(-3600),
    };

    let workspace = orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    let workspace_id = workspace.id.clone();

    // Simulate janitor cleanup
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");

    for ws in expired {
        orchestrator
            .delete_workspace(&ws.id)
            .await
            .expect("Failed to delete workspace");
    }

    // Verify workspace was deleted
    let result = orchestrator.get_workspace(&workspace_id).await;
    assert!(result.is_err(), "Workspace should have been deleted");
}

#[tokio::test]
async fn test_janitor_mixed_workspaces() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create 3 workspaces: expired, valid, no TTL
    let expired_req = CreateWorkspaceRequest {
        name: "expired-1".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(-3600),
    };

    let expired_req2 = CreateWorkspaceRequest {
        name: "expired-2".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(-7200),
    };

    let valid_req = CreateWorkspaceRequest {
        name: "valid".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(3600),
    };

    let no_ttl_req = CreateWorkspaceRequest {
        name: "no-ttl".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    orchestrator
        .create_workspace(expired_req)
        .await
        .expect("Failed to create workspace");
    orchestrator
        .create_workspace(expired_req2)
        .await
        .expect("Failed to create workspace");
    orchestrator
        .create_workspace(valid_req)
        .await
        .expect("Failed to create workspace");
    orchestrator
        .create_workspace(no_ttl_req)
        .await
        .expect("Failed to create workspace");

    // Janitor should only find 2 expired
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");

    assert_eq!(expired.len(), 2);

    let expired_names: Vec<String> = expired.iter().map(|w| w.name.clone()).collect();
    assert!(expired_names.contains(&"expired-1".to_string()));
    assert!(expired_names.contains(&"expired-2".to_string()));
}

#[tokio::test]
async fn test_ttl_boundary_conditions() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create workspace that expires in 1 second
    let req = CreateWorkspaceRequest {
        name: "expires-soon".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(1),
    };

    orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Should not be expired yet
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");
    assert_eq!(expired.len(), 0);

    // Wait for it to expire
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Should now be expired
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");
    assert_eq!(expired.len(), 1);
}

#[tokio::test]
async fn test_expires_at_is_calculated_correctly() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    let ttl_seconds = 3600; // 1 hour
    let before = Utc::now();

    let req = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(ttl_seconds),
    };

    let workspace = orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    let after = Utc::now();

    // Verify expires_at is set
    let expires_at = workspace.expires_at.expect("expires_at should be set");

    // Should be approximately now + ttl_seconds
    // Note: timestamps are stored in SQLite as seconds, so we allow a buffer
    let expected_min = before + chrono::Duration::seconds(ttl_seconds - 5);
    let expected_max = after + chrono::Duration::seconds(ttl_seconds + 5);

    assert!(
        expires_at >= expected_min && expires_at <= expected_max,
        "expires_at={}, expected between {} and {}",
        expires_at,
        expected_min,
        expected_max
    );

    // Also verify it's stored correctly
    assert_eq!(workspace.ttl_seconds, Some(ttl_seconds));
}

#[tokio::test]
async fn test_janitor_cleanup_multiple_batches() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create 10 expired workspaces
    for i in 0..10 {
        let req = CreateWorkspaceRequest {
            name: format!("expired-{}", i),
            owner: "testuser".to_string(),
            repo_url: None,
            template: None,
            provider: Some("docker".to_string()),
            ttl_seconds: Some(-3600),
        };

        orchestrator
            .create_workspace(req)
            .await
            .expect("Failed to create workspace");
    }

    // Janitor should find all 10
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");

    assert_eq!(expired.len(), 10);

    // Delete all of them
    for ws in &expired {
        orchestrator
            .delete_workspace(&ws.id)
            .await
            .expect("Failed to delete workspace");
    }

    // Should be none left
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");

    assert_eq!(expired.len(), 0);
}

#[tokio::test]
async fn test_janitor_preserves_non_expired() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool.clone());

    // Create mix of expired and valid workspaces
    for i in 0..5 {
        let req = CreateWorkspaceRequest {
            name: format!("expired-{}", i),
            owner: "testuser".to_string(),
            repo_url: None,
            template: None,
            provider: Some("docker".to_string()),
            ttl_seconds: Some(-3600),
        };
        orchestrator
            .create_workspace(req)
            .await
            .expect("Failed to create workspace");
    }

    for i in 0..5 {
        let req = CreateWorkspaceRequest {
            name: format!("valid-{}", i),
            owner: "testuser".to_string(),
            repo_url: None,
            template: None,
            provider: Some("docker".to_string()),
            ttl_seconds: Some(7200),
        };
        orchestrator
            .create_workspace(req)
            .await
            .expect("Failed to create workspace");
    }

    // Count total workspaces before cleanup
    let all_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .expect("Failed to count workspaces");
    assert_eq!(all_before.0, 10);

    // Simulate janitor cleanup
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");

    for ws in expired {
        orchestrator
            .delete_workspace(&ws.id)
            .await
            .expect("Failed to delete workspace");
    }

    // Count total workspaces after cleanup
    let all_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .expect("Failed to count workspaces");

    // Should have 5 left (the valid ones)
    assert_eq!(all_after.0, 5);
}
