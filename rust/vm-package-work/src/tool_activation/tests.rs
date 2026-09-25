use super::*;

fn database_with_release(kind: SourceKind) -> Database {
    let mut database = Database::default();
    let now = Utc::now();
    database.checkouts.insert(
        "checkout-1".into(),
        vm_packages::CheckoutRecord {
            checkout_id: "checkout-1".into(),
            package: "auth".into(),
            source_kind: kind,
            agent: "agent".into(),
            consumers: vec!["project".into()],
            task: "release".into(),
            workspace_release: true,
            source_only: false,
            initial_release: false,
            state: vm_packages::WorkflowState::Published,
            base_branch: Some("main".into()),
            base_commit: Some("a".repeat(40)),
            branch: None,
            worktree: None,
            lease: None,
            created_at: now,
            updated_at: now,
            transitions: Vec::new(),
        },
    );
    database.releases.insert(
        "rel-1".into(),
        vm_packages::ReleaseRecord {
            release_id: "rel-1".into(),
            submission_id: "sub-1".into(),
            checkout_id: "checkout-1".into(),
            package: "auth".into(),
            version: "1.0.0".into(),
            source_repository: "https://example.com/auth.git".into(),
            source_commit: "a".repeat(40),
            tag: "v1.0.0".into(),
            artifact_digest: "b".repeat(64),
            source_pushed: true,
            source_archive_digest: None,
            registry: "https://packages.example/npm".into(),
            expected_publications: Vec::new(),
            publications: Vec::new(),
            state: vm_packages::WorkflowState::Published,
            created_at: now,
            updated_at: now,
        },
    );
    database
}

#[test]
fn release_activation_excludes_language_packages() {
    let mut database = database_with_release(SourceKind::Package);

    enqueue(&mut database, "rel-1").unwrap();
    assert!(database.tool_activations.is_empty());
}

#[tokio::test]
async fn activation_plan_is_durable_idempotent_and_resumable() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    {
        let mut current = store.database.lock().await;
        let mut next = database_with_release(SourceKind::ToolCollection);
        enqueue(&mut next, "rel-1").unwrap();
        enqueue(&mut next, "rel-1").unwrap();
        store.commit(&mut current, next).await.unwrap();
    }
    let claim = ClaimToolActivationRequest {
        worker: "worker-1".into(),
        lease_seconds: 120,
    };
    let activation = store
        .claim_tool_activation(None, claim)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        activation.activation_id,
        format!("activate-{}", &vm_packages::sha256_hex("rel-1")[..32])
    );
    let plan = PlanToolActivationRequest {
        worker: "worker-1".into(),
        targets: vec![
            vm_packages::ToolActivationTargetPlan {
                target_id: "docker-running".into(),
                environment: "running-dev".into(),
                provider: "docker".into(),
                initially_running: true,
            },
            vm_packages::ToolActivationTargetPlan {
                target_id: "docker-stopped".into(),
                environment: "stopped-dev".into(),
                provider: "docker".into(),
                initially_running: false,
            },
        ],
        idempotency_key: "activation-plan-1".into(),
    };
    let planned = store
        .plan_tool_activation(&activation.activation_id, plan.clone())
        .await
        .unwrap();
    assert_eq!(planned.targets.len(), 2);
    assert_eq!(
        store
            .plan_tool_activation(&activation.activation_id, plan)
            .await
            .unwrap(),
        planned
    );
    store
        .update_tool_activation_target(
            &activation.activation_id,
            "docker-running",
            UpdateToolActivationTargetRequest {
                worker: "worker-1".into(),
                state: ToolActivationTargetState::Active,
                error: None,
                idempotency_key: "activate-running-1".into(),
            },
        )
        .await
        .unwrap();
    let waiting = store
        .finish_tool_activation(
            &activation.activation_id,
            FinishToolActivationRequest {
                worker: "worker-1".into(),
                idempotency_key: "finish-activation-1".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(waiting.state, ToolActivationState::Waiting);
    drop(store);

    let reopened = Store::open(directory.path()).await.unwrap();
    let resumed = reopened
        .claim_tool_activation(
            Some(&activation.activation_id),
            ClaimToolActivationRequest {
                worker: "worker-2".into(),
                lease_seconds: 120,
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resumed.targets.len(), 2);
    reopened
        .update_tool_activation_target(
            &activation.activation_id,
            "docker-stopped",
            UpdateToolActivationTargetRequest {
                worker: "worker-2".into(),
                state: ToolActivationTargetState::Active,
                error: None,
                idempotency_key: "activate-stopped-1".into(),
            },
        )
        .await
        .unwrap();
    let complete = reopened
        .finish_tool_activation(
            &activation.activation_id,
            FinishToolActivationRequest {
                worker: "worker-2".into(),
                idempotency_key: "finish-activation-2".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(complete.state, ToolActivationState::Complete);
}

#[tokio::test]
async fn repair_reopens_only_the_latest_empty_activation_plan() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).await.unwrap();
    let mut database = database_with_release(SourceKind::ToolCollection);
    enqueue(&mut database, "rel-1").unwrap();
    let activation_id = database.tool_activations.keys().next().unwrap().clone();
    let activation = database.tool_activations.get_mut(&activation_id).unwrap();
    activation.state = ToolActivationState::Complete;
    let mut older = activation.clone();
    older.activation_id = "activate-older-empty-plan".into();
    older.release_id = "rel-older".into();
    older.created_at -= Duration::seconds(1);
    database
        .tool_activations
        .insert(older.activation_id.clone(), older);
    database.idempotency.insert(
        format!("plan-{activation_id}"),
        IdempotencyRecord {
            fingerprint: "stale-empty-plan".into(),
            target_id: activation_id.clone(),
        },
    );
    {
        let mut current = store.database.lock().await;
        store.commit(&mut current, database).await.unwrap();
    }

    assert_eq!(store.repair_tool_activations().await.unwrap(), 1);
    let activations = store.tool_activations().await;
    assert_eq!(
        activations
            .iter()
            .find(|activation| activation.activation_id == activation_id)
            .unwrap()
            .state,
        ToolActivationState::Queued
    );
    assert_eq!(
        activations
            .iter()
            .find(|activation| activation.activation_id == "activate-older-empty-plan")
            .unwrap()
            .state,
        ToolActivationState::Complete
    );

    let claimed = store
        .claim_tool_activation(
            None,
            ClaimToolActivationRequest {
                worker: "worker-1".into(),
                lease_seconds: 120,
            },
        )
        .await
        .unwrap()
        .unwrap();
    let replanned = store
        .plan_tool_activation(
            &claimed.activation_id,
            PlanToolActivationRequest {
                worker: "worker-1".into(),
                targets: vec![vm_packages::ToolActivationTargetPlan {
                    target_id: "docker-running".into(),
                    environment: "running-dev".into(),
                    provider: "docker".into(),
                    initially_running: true,
                }],
                idempotency_key: format!("plan-{}", claimed.activation_id),
            },
        )
        .await
        .unwrap();
    assert_eq!(replanned.targets.len(), 1);
}
