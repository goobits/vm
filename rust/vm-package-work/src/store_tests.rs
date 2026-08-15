use super::*;
use crate::ImportedSubmission;
use chrono::{Duration, Utc};
use vm_packages::{
    CleanupRequest, CreateCheckout, LeaseRequest, PackageEcosystem, RegisterPackage,
    TransitionRequest, WorkflowState,
};

fn request(key: &str, agent: &str) -> CreateCheckout {
    CreateCheckout {
        package: "auth".into(),
        agent: agent.into(),
        consumers: vec!["project-b".into(), "project-a".into(), "project-a".into()],
        task: "fix token refresh".into(),
        workspace_release: false,
        lease_token: format!("lease-token-{agent}-012345678901234567890123456789"),
        idempotency_key: key.into(),
    }
}

#[tokio::test]
async fn concurrent_checkouts_are_isolated_and_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();

    let first = store
        .create_checkout(request("one", "agent-1"))
        .await
        .unwrap();
    let retry = store
        .create_checkout(request("one", "agent-1"))
        .await
        .unwrap();
    let second = store
        .create_checkout(request("two", "agent-2"))
        .await
        .unwrap();

    assert_eq!(first.checkout.checkout_id, retry.checkout.checkout_id);
    assert_eq!(retry.lease_token, first.lease_token);
    assert_ne!(first.checkout.checkout_id, second.checkout.checkout_id);
    assert_ne!(first.checkout.lease, second.checkout.lease);
    assert_eq!(first.checkout.consumers, ["project-a", "project-b"]);
}

#[tokio::test]
async fn transitions_are_validated_persisted_and_receipted() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let checkout = store
        .create_checkout(request("create", "agent-1"))
        .await
        .unwrap();
    let id = &checkout.checkout.checkout_id;

    let checked_out = store
        .transition(
            id,
            TransitionRequest {
                next: WorkflowState::CheckedOut,
                actor: "controller".into(),
                reason: "worktree ready".into(),
                commit: Some("abc123".into()),
                validation_result: None,
                idempotency_key: "transition-1".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(checked_out.state, WorkflowState::CheckedOut);
    assert!(store
        .transition(
            id,
            TransitionRequest {
                next: WorkflowState::Published,
                actor: "controller".into(),
                reason: "skip".into(),
                commit: None,
                validation_result: None,
                idempotency_key: "transition-2".into(),
            },
        )
        .await
        .is_err());

    drop(store);
    let reopened = Store::open(directory.path()).await.unwrap();
    assert_eq!(
        reopened.get_checkout(id).await.unwrap().state,
        WorkflowState::CheckedOut
    );
    assert!(
        directory
            .path()
            .join("receipts")
            .read_dir()
            .unwrap()
            .count()
            >= 3
    );
}

#[tokio::test]
async fn client_lease_tokens_make_checkout_creation_retryable() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let created = store
        .create_checkout(request("lease", "agent-1"))
        .await
        .unwrap();
    let id = &created.checkout.checkout_id;
    let token = created.lease_token.unwrap();

    assert!(store
        .renew_lease(
            id,
            LeaseRequest {
                holder: "agent-1".into(),
                lease_token: "wrong".into(),
                duration_seconds: 600,
                idempotency_key: "bad-renew".into(),
            },
        )
        .await
        .is_err());
    let renewed = store
        .renew_lease(
            id,
            LeaseRequest {
                holder: "agent-1".into(),
                lease_token: token,
                duration_seconds: 600,
                idempotency_key: "renew".into(),
            },
        )
        .await
        .unwrap();
    assert!(renewed.lease.is_some());
}

#[tokio::test]
async fn active_checkout_can_securely_reacquire_an_expired_lease() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let created = store
        .create_checkout(request("reacquire", "agent-1"))
        .await
        .unwrap();
    let id = &created.checkout.checkout_id;
    let token = created.lease_token.unwrap();
    store
        .database
        .lock()
        .await
        .checkouts
        .get_mut(id)
        .unwrap()
        .lease
        .as_mut()
        .unwrap()
        .expires_at = Utc::now() - Duration::seconds(1);
    store.expire_leases().await.unwrap();

    assert!(store
        .renew_lease(
            id,
            LeaseRequest {
                holder: "agent-1".into(),
                lease_token: "wrong-token".into(),
                duration_seconds: 600,
                idempotency_key: "wrong-reacquire-lease".into(),
            },
        )
        .await
        .is_err());

    let reacquired = store
        .renew_lease(
            id,
            LeaseRequest {
                holder: "agent-1".into(),
                lease_token: token,
                duration_seconds: 600,
                idempotency_key: "reacquire-lease".into(),
            },
        )
        .await
        .unwrap();

    assert_eq!(reacquired.lease.unwrap().holder, "agent-1");
}

#[tokio::test]
async fn catalog_retries_are_exact_and_checkout_archives_are_consumer_scoped() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let package = RegisterPackage {
        name: "auth".into(),
        ecosystem: PackageEcosystem::Cargo,
        repository: "https://example.com/auth.git".into(),
        default_branch: "main".into(),
        workspace_release: false,
    };
    assert_eq!(
        store.register_package(package.clone()).await.unwrap(),
        store.register_package(package.clone()).await.unwrap()
    );
    let mut managed = package;
    managed.workspace_release = true;
    assert!(
        store
            .register_package(managed)
            .await
            .unwrap()
            .workspace_release
    );
    assert!(store
        .register_package(RegisterPackage {
            name: "auth".into(),
            ecosystem: PackageEcosystem::Cargo,
            repository: "https://example.com/other.git".into(),
            default_branch: "main".into(),
            workspace_release: false,
        })
        .await
        .is_err());
    let catalog: vm_packages::InternalPackageCatalog = serde_json::from_slice(
        &tokio::fs::read(directory.path().join("catalog/packages.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(catalog
        .contains(&vm_packages::PackageIdentity::new(PackageEcosystem::Cargo, "auth").unwrap()));

    let checkout = store
        .create_checkout(request("scoped", "agent-1"))
        .await
        .unwrap();
    let token = checkout.lease_token.unwrap();
    assert!(store
        .authorize_lease(&checkout.checkout.checkout_id, "project-a", &token)
        .await
        .is_ok());
    assert!(store
        .authorize_lease(&checkout.checkout.checkout_id, "project-c", &token)
        .await
        .is_err());
}

#[tokio::test]
async fn terminal_checkout_cleanup_is_idempotent_without_source() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let checkout = store
        .create_checkout(request("cleanup", "agent-1"))
        .await
        .unwrap()
        .checkout;
    store
        .transition(
            &checkout.checkout_id,
            TransitionRequest {
                next: WorkflowState::Failed,
                actor: "controller".into(),
                reason: "source preparation failed".into(),
                commit: None,
                validation_result: Some("failed".into()),
                idempotency_key: "fail-cleanup".into(),
            },
        )
        .await
        .unwrap();
    let request = CleanupRequest {
        actor: "controller".into(),
        idempotency_key: "close-cleanup".into(),
    };
    assert_eq!(
        store
            .close_checkout(&checkout.checkout_id, request.clone())
            .await
            .unwrap()
            .state,
        WorkflowState::Closed
    );
    assert_eq!(
        store
            .close_checkout(&checkout.checkout_id, request)
            .await
            .unwrap()
            .state,
        WorkflowState::Closed
    );
}

#[tokio::test]
async fn checkout_transition_keeps_submission_in_sync_when_cancelled() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let checkout = store
        .create_checkout(request("cancel", "agent-1"))
        .await
        .unwrap()
        .checkout;
    store
        .record_source(
            &checkout.checkout_id,
            "main".into(),
            "a".repeat(40),
            "agents/agent-1/cancel".into(),
            "/data/agents/cancel/source".into(),
        )
        .await
        .unwrap();
    store
        .transition(
            &checkout.checkout_id,
            TransitionRequest {
                next: WorkflowState::Active,
                actor: "agent-1".into(),
                reason: "attached".into(),
                commit: None,
                validation_result: None,
                idempotency_key: "activate-cancel".into(),
            },
        )
        .await
        .unwrap();
    let submission = store
        .record_submission(
            &checkout.checkout_id,
            ImportedSubmission {
                submitted_commit: "b".repeat(40),
                diff_digest: "c".repeat(64),
            },
        )
        .await
        .unwrap();
    store
        .transition(
            &checkout.checkout_id,
            TransitionRequest {
                next: WorkflowState::NeedsChanges,
                actor: "reviewer".into(),
                reason: "revise".into(),
                commit: None,
                validation_result: Some("needs_changes".into()),
                idempotency_key: "revise-cancel".into(),
            },
        )
        .await
        .unwrap();
    let cancelled = store
        .transition(
            &checkout.checkout_id,
            TransitionRequest {
                next: WorkflowState::Cancelled,
                actor: "controller".into(),
                reason: "cancelled".into(),
                commit: None,
                validation_result: Some("cancelled".into()),
                idempotency_key: "cancel-checkout".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(cancelled.state, WorkflowState::Cancelled);
    assert!(cancelled.lease.is_none());
    assert_eq!(
        store
            .submission(&submission.submission_id)
            .await
            .unwrap()
            .state,
        WorkflowState::Cancelled
    );
}
