//! Integration tests for the provisioner background task
//!
//! Tests that the provisioner picks up "creating" workspaces and provisions them.
//! Docker-dependent tests are marked with #[ignore].

use sqlx::SqlitePool;
use vm_orchestrator::{CreateWorkspaceRequest, WorkspaceOrchestrator, WorkspaceStatus};

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
async fn test_provisioner_picks_up_creating_workspaces() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create a workspace (should be in Creating status)
    let req = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    let workspace = orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Verify it's in Creating status
    match workspace.status {
        WorkspaceStatus::Creating => {}
        _ => panic!("Expected Creating status"),
    }

    // Get all creating workspaces (this is what the provisioner does)
    let creating = orchestrator
        .get_workspaces_by_status(WorkspaceStatus::Creating)
        .await
        .expect("Failed to get creating workspaces");

    assert_eq!(creating.len(), 1);
    assert_eq!(creating[0].id, workspace.id);
}

#[tokio::test]
async fn test_successful_provisioning_updates_status() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create a workspace
    let req = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    let workspace = orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Simulate successful provisioning
    let provider_id = "container-abc123".to_string();
    let connection_info = serde_json::json!({
        "container_id": "container-abc123",
        "status": "running",
        "ssh_command": "vm ssh test-workspace"
    });

    orchestrator
        .update_workspace_status(
            &workspace.id,
            WorkspaceStatus::Running,
            Some(provider_id.clone()),
            Some(connection_info.clone()),
            None,
        )
        .await
        .expect("Failed to update workspace status");

    // Verify the workspace is now running
    let updated = orchestrator
        .get_workspace(&workspace.id)
        .await
        .expect("Failed to get workspace");

    match updated.status {
        WorkspaceStatus::Running => {}
        _ => panic!("Expected Running status"),
    }
    assert_eq!(updated.provider_id, Some(provider_id));
    assert_eq!(updated.connection_info, Some(connection_info));
    assert!(updated.error_message.is_none());
}

#[tokio::test]
async fn test_failed_provisioning_updates_status() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create a workspace
    let req = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    let workspace = orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Simulate failed provisioning
    let error_message = "Failed to pull Docker image: image not found".to_string();

    orchestrator
        .update_workspace_status(
            &workspace.id,
            WorkspaceStatus::Failed,
            None,
            None,
            Some(error_message.clone()),
        )
        .await
        .expect("Failed to update workspace status");

    // Verify the workspace is now failed
    let updated = orchestrator
        .get_workspace(&workspace.id)
        .await
        .expect("Failed to get workspace");

    match updated.status {
        WorkspaceStatus::Failed => {}
        _ => panic!("Expected Failed status"),
    }
    assert_eq!(updated.error_message, Some(error_message));
    assert!(updated.provider_id.is_none());
}

#[tokio::test]
async fn test_provisioner_ignores_non_creating_workspaces() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create a workspace
    let req = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    let workspace = orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Update to Running status
    orchestrator
        .update_workspace_status(
            &workspace.id,
            WorkspaceStatus::Running,
            Some("container-123".to_string()),
            None,
            None,
        )
        .await
        .expect("Failed to update status");

    // Provisioner should not pick this up
    let creating = orchestrator
        .get_workspaces_by_status(WorkspaceStatus::Creating)
        .await
        .expect("Failed to get creating workspaces");

    assert_eq!(creating.len(), 0);
}

#[tokio::test]
async fn test_multiple_workspaces_provisioning() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create multiple workspaces
    for i in 0..3 {
        let req = CreateWorkspaceRequest {
            name: format!("workspace-{}", i),
            owner: "testuser".to_string(),
            repo_url: None,
            template: None,
            provider: Some("docker".to_string()),
            ttl_seconds: None,
        };

        orchestrator
            .create_workspace(req)
            .await
            .expect("Failed to create workspace");
    }

    // All should be in Creating status
    let creating = orchestrator
        .get_workspaces_by_status(WorkspaceStatus::Creating)
        .await
        .expect("Failed to get creating workspaces");

    assert_eq!(creating.len(), 3);

    // Simulate provisioning one
    orchestrator
        .update_workspace_status(
            &creating[0].id,
            WorkspaceStatus::Running,
            Some("container-123".to_string()),
            None,
            None,
        )
        .await
        .expect("Failed to update status");

    // Should now have 2 creating workspaces
    let creating = orchestrator
        .get_workspaces_by_status(WorkspaceStatus::Creating)
        .await
        .expect("Failed to get creating workspaces");

    assert_eq!(creating.len(), 2);
}

// Note: Real Docker provisioning tests would require a running Docker daemon
// and are better suited for end-to-end testing. The tests above verify the
// orchestrator logic independent of the actual provisioning implementation.
