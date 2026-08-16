use std::sync::Arc;

use axum::http::{header, StatusCode};
use axum_test::TestServer;

use super::{router, WorkCredentials};
use crate::Store;

fn checkout() -> serde_json::Value {
    serde_json::json!({
        "package": "auth", "agent": "agent-1", "consumers": ["project-a"],
        "task": "refresh tokens", "lease_token": "lease-token-012345678901234567890123456789",
        "idempotency_key": "create-1"
    })
}

#[tokio::test]
async fn health_is_public_and_workflow_access_is_scoped() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(directory.path()).await.unwrap());
    let server = TestServer::new(router(
        store.clone(),
        WorkCredentials::new(
            "read",
            "controller",
            "reviewer",
            "build",
            "release",
            "rollout",
            "agent-signing-key-012345678901234567890123456789",
        ),
    ));

    assert_eq!(server.get("/health").await.status_code(), StatusCode::OK);
    assert_eq!(
        server.get("/v1/checkouts").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    let agent = vm_packages::issue_agent_capability(
        "agent-signing-key-012345678901234567890123456789",
        "project-a",
    )
    .unwrap();
    assert_eq!(
        server
            .get("/v1/checkouts")
            .add_header(header::AUTHORIZATION, "Bearer read")
            .await
            .status_code(),
        StatusCode::OK
    );
    assert_eq!(
        server
            .post("/v1/checkouts")
            .add_header(header::AUTHORIZATION, "Bearer read")
            .json(&checkout())
            .await
            .status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server.get("/v1/jobs/review/next").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .get("/v1/jobs/review/next")
            .add_header(header::AUTHORIZATION, "Bearer reviewer")
            .await
            .status_code(),
        StatusCode::OK
    );
    assert_eq!(
        server
            .get("/v1/jobs/review/next")
            .add_header(header::AUTHORIZATION, "Bearer release")
            .await
            .status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .get("/v1/jobs/build/next")
            .add_header(header::AUTHORIZATION, "Bearer build")
            .await
            .status_code(),
        StatusCode::OK
    );
    assert_eq!(
        server
            .get("/v1/jobs/build/next")
            .add_header(header::AUTHORIZATION, "Bearer release")
            .await
            .status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .get("/v1/jobs/release/next")
            .add_header(header::AUTHORIZATION, "Bearer build")
            .await
            .status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .get("/v1/jobs/rollout/reconcile")
            .add_header(header::AUTHORIZATION, "Bearer rollout")
            .await
            .status_code(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        server
            .post("/v1/jobs/rollout/reconcile")
            .add_header(header::AUTHORIZATION, "Bearer reviewer")
            .await
            .status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .post("/v1/jobs/rollout/reconcile")
            .add_header(header::AUTHORIZATION, "Bearer rollout")
            .await
            .status_code(),
        StatusCode::OK
    );
    assert_eq!(
        server
            .get("/v1/checkouts")
            .add_header(header::AUTHORIZATION, "Bearer release")
            .await
            .status_code(),
        StatusCode::OK
    );
    assert_eq!(server.post("/v1/packages").add_header(header::AUTHORIZATION, "Bearer release").json(&serde_json::json!({
        "name": "blocked", "ecosystem": "cargo", "repository": "https://example.com/blocked.git", "default_branch": "main"
    })).await.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        server
            .post("/v1/checkouts")
            .add_header(header::AUTHORIZATION, format!("Bearer {agent}"))
            .json(&checkout())
            .await
            .status_code(),
        StatusCode::NOT_FOUND
    );
    let mut other_checkout = checkout();
    other_checkout["consumers"] = serde_json::json!(["project-b"]);
    assert_eq!(
        server
            .post("/v1/checkouts")
            .add_header(header::AUTHORIZATION, format!("Bearer {agent}"))
            .json(&other_checkout)
            .await
            .status_code(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(server.post("/v1/packages").add_header(header::AUTHORIZATION, "Bearer controller").json(&serde_json::json!({
        "name": "auth", "ecosystem": "cargo", "repository": "https://example.com/auth.git", "default_branch": "main"
    })).await.status_code(), StatusCode::CREATED);
    let project_a = store
        .create_checkout(serde_json::from_value(checkout()).unwrap())
        .await
        .unwrap()
        .checkout;
    let mut project_b_request = checkout();
    project_b_request["consumers"] = serde_json::json!(["project-b"]);
    project_b_request["lease_token"] =
        serde_json::json!("other-lease-token-012345678901234567890123456789");
    project_b_request["idempotency_key"] = serde_json::json!("create-2");
    let project_b = store
        .create_checkout(serde_json::from_value(project_b_request).unwrap())
        .await
        .unwrap()
        .checkout;
    let scoped_response = server
        .get("/v1/checkouts")
        .add_header(header::AUTHORIZATION, format!("Bearer {agent}"))
        .await;
    assert_eq!(scoped_response.status_code(), StatusCode::OK);
    let scoped: Vec<vm_packages::CheckoutRecord> = scoped_response.json();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].checkout_id, project_a.checkout_id);
    assert_eq!(
        server
            .get(&format!("/v1/checkouts/{}", project_b.checkout_id))
            .add_header(header::AUTHORIZATION, format!("Bearer {agent}"))
            .await
            .status_code(),
        StatusCode::UNAUTHORIZED
    );
    let response = server
        .get("/v1/checkouts")
        .add_header(header::AUTHORIZATION, "Bearer read")
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.json::<Vec<vm_packages::CheckoutRecord>>().len(), 2);
    assert_eq!(server.post("/v1/tools").add_header(header::AUTHORIZATION, "Bearer release").json(&serde_json::json!({
        "name": "codex", "kind": "binary", "repository": "https://example.com/codex.git", "default_branch": "main"
    })).await.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(server.post("/v1/tools").add_header(header::AUTHORIZATION, "Bearer controller").json(&serde_json::json!({
        "name": "codex", "kind": "binary", "repository": "https://example.com/codex.git", "default_branch": "main"
    })).await.status_code(), StatusCode::CREATED);
    assert_eq!(
        server
            .get("/v1/tools/index?target=linux-arm64")
            .add_header(header::AUTHORIZATION, "Bearer read")
            .await
            .status_code(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn guest_checkout_identity_is_derived_from_its_capability() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(directory.path()).await.unwrap());
    let server = TestServer::new(router(
        store,
        WorkCredentials::new(
            "read",
            "controller",
            "reviewer",
            "build",
            "release",
            "rollout",
            "agent-signing-key-012345678901234567890123456789",
        ),
    ));
    assert_eq!(
        server
            .post("/v1/packages")
            .add_header(header::AUTHORIZATION, "Bearer controller")
            .json(&serde_json::json!({
                "name": "workspace-auth",
                "ecosystem": "cargo",
                "repository": "https://example.com/workspace-auth.git",
                "default_branch": "main",
                "workspace_release": true
            }))
            .await
            .status_code(),
        StatusCode::CREATED
    );
    let agent = vm_packages::issue_agent_capability(
        "agent-signing-key-012345678901234567890123456789",
        "project-a",
    )
    .unwrap();
    let request = serde_json::json!({
        "package": "workspace-auth",
        "agent": "spoofed-agent",
        "consumers": ["project-b"],
        "task": "spoofed task",
        "workspace_release": true,
        "lease_token": "lease-token-012345678901234567890123456789",
        "idempotency_key": "workspace-create-1"
    });
    let response = server
        .post("/v1/checkouts")
        .add_header(header::AUTHORIZATION, format!("Bearer {agent}"))
        .json(&request)
        .await;
    assert_eq!(response.status_code(), StatusCode::CREATED);
    let created: vm_packages::CheckoutLease = response.json();
    assert_eq!(created.checkout.agent, "project-a");
    assert_eq!(created.checkout.consumers, ["project-a"]);
    assert_eq!(created.checkout.task, "managed guest package work");

    let mut retry = request;
    retry["agent"] = serde_json::json!("another-spoof");
    retry["task"] = serde_json::json!("another task");
    retry["lease_token"] = serde_json::json!("rotated-token-012345678901234567890123456789");
    retry["idempotency_key"] = serde_json::json!("workspace-create-2");
    let response = server
        .post("/v1/checkouts")
        .add_header(header::AUTHORIZATION, format!("Bearer {agent}"))
        .json(&retry)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let resumed: vm_packages::CheckoutLease = response.json();
    assert_eq!(resumed.checkout.checkout_id, created.checkout.checkout_id);
    assert_eq!(resumed.checkout.agent, "project-a");

    let response = server
        .post(&format!(
            "/v1/checkouts/{}/transition",
            resumed.checkout.checkout_id
        ))
        .add_header(header::AUTHORIZATION, format!("Bearer {agent}"))
        .json(&serde_json::json!({
            "next": "cancelled",
            "actor": "spoofed-agent",
            "reason": "cancel test",
            "commit": null,
            "validation_result": "cancelled",
            "idempotency_key": "workspace-cancel-1"
        }))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let cancelled: vm_packages::CheckoutRecord = response.json();
    assert_eq!(cancelled.state, vm_packages::WorkflowState::Closed);
    assert!(cancelled.transitions.iter().any(|transition| {
        transition.next == vm_packages::WorkflowState::Cancelled && transition.actor == "project-a"
    }));

    retry["lease_token"] = serde_json::json!("terminal-retry-012345678901234567890123456789");
    retry["idempotency_key"] = serde_json::json!("workspace-create-3");
    let response = server
        .post("/v1/checkouts")
        .add_header(header::AUTHORIZATION, format!("Bearer {agent}"))
        .json(&retry)
        .await;
    assert_eq!(response.status_code(), StatusCode::CREATED);
    let after_terminal: vm_packages::CheckoutLease = response.json();
    assert_ne!(
        after_terminal.checkout.checkout_id,
        created.checkout.checkout_id
    );
}
