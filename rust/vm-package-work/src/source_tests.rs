use std::process::{Command as StdCommand, Stdio};

use super::*;
use vm_packages::{
    CreateCheckout, CreateRollout, IntegrationRecord, PackageEcosystem, PublicationRecord,
    RegisterConsumer, RegisterPackage, ReleaseRecord, RolloutState,
};

fn git(repository: &Path, args: &[&str]) {
    assert!(StdCommand::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success());
}

#[tokio::test]
async fn package_checkout_lifecycle_stays_inside_managed_agent_storage() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path().join("repository");
    std::fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "--initial-branch", "main"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    git(&repository, &["config", "user.name", "Test"]);
    std::fs::write(
        repository.join("Cargo.toml"),
        "[package]\nname='auth'\nversion='1.0.0'\n",
    )
    .unwrap();
    git(&repository, &["add", "Cargo.toml"]);
    git(&repository, &["commit", "-m", "initial"]);

    let data = directory.path().join("data");
    let store = Store::open(&data).await.unwrap();
    store
        .register_package(RegisterPackage {
            name: "auth".into(),
            ecosystem: PackageEcosystem::Cargo,
            repository: url::Url::from_file_path(&repository).unwrap().into(),
            default_branch: "main".into(),
            ci_registry: None,
        })
        .await
        .unwrap();
    let checkout = store
        .create_checkout(CreateCheckout {
            package: "auth".into(),
            agent: "agent-1".into(),
            consumers: vec!["project-a".into()],
            task: "change auth".into(),
            idempotency_key: "checkout-1".into(),
        })
        .await
        .unwrap();
    let source = SourceManager::new(&data);
    let prepared = source.prepare(&store, &checkout).await.unwrap();

    assert_eq!(prepared.state, WorkflowState::CheckedOut);
    assert!(prepared
        .worktree
        .as_deref()
        .unwrap()
        .starts_with(data.join("agents").to_str().unwrap()));
    let bundle = source.archive(&prepared).await.unwrap();
    let consumer = directory.path().join("consumer");
    assert!(StdCommand::new("git")
        .args(["clone"])
        .arg(&bundle)
        .arg(&consumer)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success());
    git(&consumer, &["switch", prepared.branch.as_deref().unwrap()]);
    let branch = StdCommand::new("git")
        .arg("-C")
        .arg(&consumer)
        .args(["branch", "--show-current"])
        .output()
        .unwrap();
    assert!(branch.status.success());
    assert_eq!(
        String::from_utf8(branch.stdout).unwrap().trim(),
        prepared.branch.as_deref().unwrap()
    );

    let active = store
        .transition(
            &prepared.checkout_id,
            vm_packages::TransitionRequest {
                next: WorkflowState::Active,
                actor: "agent-1".into(),
                reason: "consumer attached".into(),
                commit: prepared.base_commit.clone(),
                validation_result: None,
                idempotency_key: "active".into(),
            },
        )
        .await
        .unwrap();
    git(&consumer, &["config", "user.email", "agent@example.com"]);
    git(&consumer, &["config", "user.name", "Agent"]);
    std::fs::write(
        consumer.join("Cargo.toml"),
        "[package]\nname='auth'\nversion='1.0.1'\n",
    )
    .unwrap();
    git(&consumer, &["add", "Cargo.toml"]);
    git(&consumer, &["commit", "-m", "update auth"]);
    let submitted_bundle = directory.path().join("submitted.bundle");
    git(
        &consumer,
        &[
            "bundle",
            "create",
            submitted_bundle.to_str().unwrap(),
            "--all",
        ],
    );
    let mut submission = source
        .import_submission(&store, &active, &submitted_bundle)
        .await
        .unwrap();
    assert_eq!(submission.state, WorkflowState::Submitted);
    assert_ne!(submission.base_commit, submission.submitted_commit);

    let integration_root = data
        .join("agents")
        .join(&active.checkout_id)
        .join("integrations")
        .join(&submission.submission_id);
    let integration_source = integration_root.join("source");
    std::fs::create_dir_all(&integration_source).unwrap();
    std::fs::write(integration_source.join("temporary"), "worktree").unwrap();
    std::fs::write(
        integration_root.join("integration.bundle"),
        "release bundle",
    )
    .unwrap();
    submission.integration = Some(IntegrationRecord {
        canonical_commit: submission.base_commit.clone(),
        integration_commit: submission.submitted_commit.clone(),
        strategy: "rebase".into(),
        worktree: integration_source.to_string_lossy().into_owned(),
        validation: None,
        timestamp: chrono::Utc::now(),
    });

    source
        .compact_integrated_checkout(&submission)
        .await
        .unwrap();
    source
        .compact_integrated_checkout(&submission)
        .await
        .unwrap();
    assert!(!data
        .join("agents")
        .join(&active.checkout_id)
        .join("source")
        .exists());
    assert!(!integration_source.exists());
    assert!(integration_root.join("integration.bundle").is_file());
    assert!(repository.join(".git").is_dir());
    assert!(repository.join("Cargo.toml").is_file());
    assert!(data.join("sources").is_dir());

    source.cleanup_checkout(&active).await.unwrap();
    source.cleanup_checkout(&active).await.unwrap();
    assert!(!data.join("agents").join(&active.checkout_id).exists());
    assert!(repository.join(".git").is_dir());
    assert!(repository.join("Cargo.toml").is_file());
    assert!(data.join("sources").is_dir());
}

#[tokio::test]
async fn consumer_rollout_isolated_bundle_pushes_only_its_upgrade_branch() {
    let directory = tempfile::tempdir().unwrap();
    let package_repository = directory.path().join("package");
    std::fs::create_dir(&package_repository).unwrap();
    git(&package_repository, &["init", "--initial-branch", "main"]);
    git(
        &package_repository,
        &["config", "user.email", "test@example.com"],
    );
    git(&package_repository, &["config", "user.name", "Test"]);
    std::fs::write(
        package_repository.join("Cargo.toml"),
        "[package]\nname='auth'\nversion='1.1.0'\n",
    )
    .unwrap();
    git(&package_repository, &["add", "Cargo.toml"]);
    git(&package_repository, &["commit", "-m", "auth release"]);

    let consumer_repository = directory.path().join("consumer-repository");
    std::fs::create_dir(&consumer_repository).unwrap();
    git(&consumer_repository, &["init", "--initial-branch", "main"]);
    git(
        &consumer_repository,
        &["config", "user.email", "test@example.com"],
    );
    git(&consumer_repository, &["config", "user.name", "Test"]);
    std::fs::write(
        consumer_repository.join("Cargo.toml"),
        "[package]\nname='app'\nversion='1.0.0'\n[dependencies]\nauth='1.0.0'\n",
    )
    .unwrap();
    git(&consumer_repository, &["add", "Cargo.toml"]);
    git(&consumer_repository, &["commit", "-m", "initial consumer"]);

    let data = directory.path().join("data");
    let store = Store::open(&data).await.unwrap();
    store
        .register_package(RegisterPackage {
            name: "auth".into(),
            ecosystem: PackageEcosystem::Cargo,
            repository: url::Url::from_file_path(&package_repository)
                .unwrap()
                .into(),
            default_branch: "main".into(),
            ci_registry: None,
        })
        .await
        .unwrap();
    let now = chrono::Utc::now();
    store.database.lock().await.releases.insert(
        "rel-auth".into(),
        ReleaseRecord {
            release_id: "rel-auth".into(),
            submission_id: "sub-auth".into(),
            checkout_id: "checkout-auth".into(),
            package: "auth".into(),
            version: "1.1.0".into(),
            source_repository: url::Url::from_file_path(&package_repository)
                .unwrap()
                .into(),
            source_commit: "a".repeat(40),
            tag: "v1.1.0".into(),
            artifact_digest: "b".repeat(64),
            source_pushed: true,
            expected_registries: vec!["https://packages.example/cargo/".into()],
            publications: vec![PublicationRecord {
                registry: "https://packages.example/cargo/".into(),
                artifact_digest: "b".repeat(64),
                published_at: now,
            }],
            state: WorkflowState::Published,
            created_at: now,
            updated_at: now,
        },
    );
    store
        .register_consumer(RegisterConsumer {
            name: "app".into(),
            repository: url::Url::from_file_path(&consumer_repository)
                .unwrap()
                .into(),
            default_branch: "main".into(),
            dependencies: std::collections::BTreeMap::from([("auth".into(), "1.0.0".into())]),
        })
        .await
        .unwrap();
    let rollout = store
        .create_rollout(CreateRollout {
            package: "auth".into(),
            version: "1.1.0".into(),
            consumer: "app".into(),
            actor: "controller".into(),
            idempotency_key: "create-rollout-source".into(),
        })
        .await
        .unwrap();
    let source = SourceManager::new(&data);
    let rollout = source.prepare_rollout(&store, &rollout).await.unwrap();
    assert_eq!(rollout.state, RolloutState::Active);
    let bundle = source.rollout_bundle(&rollout).await.unwrap();
    let agent = directory.path().join("rollout-agent");
    assert!(StdCommand::new("git")
        .arg("clone")
        .arg(&bundle)
        .arg(&agent)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success());
    git(&agent, &["switch", rollout.branch.as_deref().unwrap()]);
    git(&agent, &["config", "user.email", "rollout@example.com"]);
    git(&agent, &["config", "user.name", "Rollout"]);
    std::fs::write(
        agent.join("Cargo.toml"),
        "[package]\nname='app'\nversion='1.0.0'\n[dependencies]\nauth='1.1.0'\n",
    )
    .unwrap();
    git(&agent, &["add", "Cargo.toml"]);
    git(&agent, &["commit", "-m", "update auth"]);
    let submitted = directory.path().join("rollout-submitted.bundle");
    git(
        &agent,
        &["bundle", "create", submitted.to_str().unwrap(), "--all"],
    );
    let rollout = source
        .import_rollout(&store, &rollout, &submitted)
        .await
        .unwrap();
    assert_eq!(rollout.state, RolloutState::Validating);
    source.push_rollout(&store, &rollout).await.unwrap();
    let remote = StdCommand::new("git")
        .arg("-C")
        .arg(&consumer_repository)
        .args(["rev-parse", rollout.branch.as_deref().unwrap()])
        .output()
        .unwrap();
    assert!(remote.status.success());
    assert_eq!(
        String::from_utf8(remote.stdout).unwrap().trim(),
        rollout.submitted_commit.as_deref().unwrap()
    );
    source.cleanup_rollout(&rollout).await.unwrap();
    source.cleanup_rollout(&rollout).await.unwrap();
    assert!(!data.join("rollouts").join(&rollout.rollout_id).exists());
}
