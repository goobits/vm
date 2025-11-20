//! Integration tests for vm-orchestrator
//!
//! Tests core orchestrator functionality including workspace creation,
//! status tracking, TTL expiration, and operations recording.

use chrono::Utc;
use vm_orchestrator::test_utils::create_test_db;
use vm_orchestrator::{
    CreateWorkspaceRequest, WorkspaceFilters, WorkspaceOrchestrator, WorkspaceStatus,
};

#[tokio::test]
async fn test_create_workspace() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    let req = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: Some("https://github.com/test/repo".to_string()),
        template: Some("nodejs".to_string()),
        provider: Some("docker".to_string()),
        ttl_seconds: Some(3600),
    };

    let workspace = orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Verify workspace was created correctly
    assert_eq!(workspace.name, "test-workspace");
    assert_eq!(workspace.owner, "testuser");
    assert_eq!(workspace.provider, "docker");
    assert_eq!(
        workspace.repo_url,
        Some("https://github.com/test/repo".to_string())
    );
    assert_eq!(workspace.template, Some("nodejs".to_string()));
    assert_eq!(workspace.ttl_seconds, Some(3600));
    assert!(workspace.expires_at.is_some());

    // Should start in Creating status
    match workspace.status {
        WorkspaceStatus::Creating => {}
        _ => panic!("Expected Creating status, got {:?}", workspace.status),
    }
}

#[tokio::test]
async fn test_list_workspaces_with_filters() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create workspaces for different users
    let req1 = CreateWorkspaceRequest {
        name: "workspace1".to_string(),
        owner: "alice".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    let req2 = CreateWorkspaceRequest {
        name: "workspace2".to_string(),
        owner: "bob".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    let req3 = CreateWorkspaceRequest {
        name: "workspace3".to_string(),
        owner: "alice".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    orchestrator
        .create_workspace(req1)
        .await
        .expect("Failed to create workspace1");
    orchestrator
        .create_workspace(req2)
        .await
        .expect("Failed to create workspace2");
    orchestrator
        .create_workspace(req3)
        .await
        .expect("Failed to create workspace3");

    // List all workspaces for alice
    let filters = WorkspaceFilters {
        owner: Some("alice".to_string()),
        status: None,
    };
    let alice_workspaces = orchestrator
        .list_workspaces(filters)
        .await
        .expect("Failed to list workspaces");

    assert_eq!(alice_workspaces.len(), 2);
    assert!(alice_workspaces.iter().all(|w| w.owner == "alice"));

    // List all workspaces for bob
    let filters = WorkspaceFilters {
        owner: Some("bob".to_string()),
        status: None,
    };
    let bob_workspaces = orchestrator
        .list_workspaces(filters)
        .await
        .expect("Failed to list workspaces");

    assert_eq!(bob_workspaces.len(), 1);
    assert_eq!(bob_workspaces[0].owner, "bob");
}

#[tokio::test]
async fn test_update_workspace_status() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

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

    // Update to running with provider_id and connection_info
    let provider_id = "container-abc123".to_string();
    let connection_info = serde_json::json!({
        "container_id": "container-abc123",
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

    // Fetch and verify the update
    let updated_workspace = orchestrator
        .get_workspace(&workspace.id)
        .await
        .expect("Failed to get workspace");

    match updated_workspace.status {
        WorkspaceStatus::Running => {}
        _ => panic!(
            "Expected Running status, got {:?}",
            updated_workspace.status
        ),
    }
    assert_eq!(updated_workspace.provider_id, Some(provider_id));
    assert_eq!(updated_workspace.connection_info, Some(connection_info));
}

#[tokio::test]
async fn test_update_workspace_status_to_failed() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

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

    // Update to failed with error message
    let error_message = "Failed to pull Docker image".to_string();

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

    // Fetch and verify the update
    let updated_workspace = orchestrator
        .get_workspace(&workspace.id)
        .await
        .expect("Failed to get workspace");

    match updated_workspace.status {
        WorkspaceStatus::Failed => {}
        _ => panic!("Expected Failed status, got {:?}", updated_workspace.status),
    }
    assert_eq!(updated_workspace.error_message, Some(error_message));
}

#[tokio::test]
async fn test_get_expired_workspaces() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create workspace that expires in the past
    let req_expired = CreateWorkspaceRequest {
        name: "expired-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(-3600), // Expired 1 hour ago
    };

    // Create workspace that expires in the future
    let req_valid = CreateWorkspaceRequest {
        name: "valid-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: Some(3600), // Expires in 1 hour
    };

    // Create workspace with no TTL
    let req_no_ttl = CreateWorkspaceRequest {
        name: "no-ttl-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None,
    };

    orchestrator
        .create_workspace(req_expired)
        .await
        .expect("Failed to create expired workspace");
    orchestrator
        .create_workspace(req_valid)
        .await
        .expect("Failed to create valid workspace");
    orchestrator
        .create_workspace(req_no_ttl)
        .await
        .expect("Failed to create no-ttl workspace");

    // Get expired workspaces
    let expired = orchestrator
        .get_expired_workspaces()
        .await
        .expect("Failed to get expired workspaces");

    // Should only return the expired one
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].name, "expired-workspace");
}

#[tokio::test]
async fn test_record_operation() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool.clone());

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

    // Creating a workspace should automatically record a create operation
    // Let's verify it exists
    let operations = sqlx::query("SELECT * FROM operations WHERE workspace_id = ?")
        .bind(&workspace.id)
        .fetch_all(&pool)
        .await
        .expect("Failed to query operations");

    assert_eq!(operations.len(), 1);
}

#[tokio::test]
async fn test_delete_workspace() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

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

    let workspace_id = workspace.id.clone();

    // Delete the workspace
    orchestrator
        .delete_workspace(&workspace_id)
        .await
        .expect("Failed to delete workspace");

    // Verify it no longer exists
    let result = orchestrator.get_workspace(&workspace_id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_workspaces_by_status() {
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

    // Update one to Running
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

    // Now should have 2 creating and 1 running
    let creating = orchestrator
        .get_workspaces_by_status(WorkspaceStatus::Creating)
        .await
        .expect("Failed to get creating workspaces");
    assert_eq!(creating.len(), 2);

    let running = orchestrator
        .get_workspaces_by_status(WorkspaceStatus::Running)
        .await
        .expect("Failed to get running workspaces");
    assert_eq!(running.len(), 1);
}

#[tokio::test]
async fn test_workspace_default_provider() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    // Create workspace without specifying provider
    let req = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: None, // Don't specify provider
        ttl_seconds: None,
    };

    let workspace = orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // Should default to docker
    assert_eq!(workspace.provider, "docker");
}

#[tokio::test]
async fn test_ttl_calculation() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    let ttl_seconds = 7200; // 2 hours
    let before_creation = Utc::now();

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

    let after_creation = Utc::now();

    // Verify expires_at is set correctly
    let expires_at = workspace.expires_at.expect("expires_at should be set");

    // expires_at should be approximately created_at + ttl_seconds
    // Allow some buffer for test execution time
    let expected_min = before_creation + chrono::Duration::seconds(ttl_seconds - 5);
    let expected_max = after_creation + chrono::Duration::seconds(ttl_seconds + 5);

    assert!(
        expires_at >= expected_min && expires_at <= expected_max,
        "expires_at={}, expected between {} and {}",
        expires_at,
        expected_min,
        expected_max
    );
}

#[tokio::test]
async fn test_workspace_without_ttl() {
    let pool = create_test_db().await;
    let orchestrator = WorkspaceOrchestrator::new(pool);

    let req = CreateWorkspaceRequest {
        name: "test-workspace".to_string(),
        owner: "testuser".to_string(),
        repo_url: None,
        template: None,
        provider: Some("docker".to_string()),
        ttl_seconds: None, // No TTL
    };

    let workspace = orchestrator
        .create_workspace(req)
        .await
        .expect("Failed to create workspace");

    // expires_at should be None
    assert!(workspace.expires_at.is_none());
    assert!(workspace.ttl_seconds.is_none());
}
