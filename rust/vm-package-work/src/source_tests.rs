use std::process::{Command as StdCommand, Stdio};

use super::*;
use crate::Store;
use vm_packages::{
    CheckOutcome, CreateCheckout, CreateRollout, IntegrationRecord, IntegrationRequest,
    PackageEcosystem, PublicApiDiff, PublicationRecord, RegisterConsumer, RegisterPackage,
    RegisterTool, ReleaseRecord, ReviewDecision, ReviewRequest, RolloutState, SourceKind,
    SubmissionRecord, ToolBuild, ToolBuildSource, ToolKind, ToolSourceManifest, ValidationRequest,
    VersionRecommendation, WorkflowState,
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

fn git_output(repository: &Path, args: &[&str]) -> String {
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[tokio::test]
async fn binary_build_sources_are_declared_registered_and_immutable() {
    let directory = tempfile::tempdir().unwrap();
    let hif = directory.path().join("hif");
    std::fs::create_dir(&hif).unwrap();
    git(&hif, &["init", "--initial-branch", "main"]);
    git(&hif, &["config", "user.email", "test@example.com"]);
    git(&hif, &["config", "user.name", "Test"]);
    std::fs::write(hif.join("source.txt"), "immutable input\n").unwrap();
    git(&hif, &["add", "source.txt"]);
    git(&hif, &["commit", "-m", "initial"]);
    let hif_commit = git_output(&hif, &["rev-parse", "HEAD"]);

    let data = directory.path().join("data");
    let store = Store::open(&data).await.unwrap();
    store
        .register_tool(RegisterTool {
            name: "hif".into(),
            kind: ToolKind::Binary,
            repository: url::Url::from_file_path(&hif).unwrap().into(),
            default_branch: "main".into(),
            build_sources: Vec::new(),
            workspace_release: false,
        })
        .await
        .unwrap();
    store
        .register_tool(RegisterTool {
            name: "hqa".into(),
            kind: ToolKind::Binary,
            repository: url::Url::from_file_path(directory.path().join("hqa"))
                .unwrap()
                .into(),
            default_branch: "main".into(),
            build_sources: vec!["hif".into()],
            workspace_release: false,
        })
        .await
        .unwrap();

    let hqa = directory.path().join("hqa");
    std::fs::create_dir(&hqa).unwrap();
    git(&hqa, &["init", "--initial-branch", "main"]);
    git(&hqa, &["config", "user.email", "test@example.com"]);
    git(&hqa, &["config", "user.name", "Test"]);
    let manifest = ToolSourceManifest {
        schema: 1,
        kind: ToolKind::Binary,
        version: Some("5.0.0".into()),
        build_sources: vec![ToolBuildSource {
            name: "hif".into(),
            commit: hif_commit.clone(),
        }],
        builds: vec![ToolBuild {
            target: "linux-arm64".into(),
            command: vec!["./build-tool".into()],
            archive: "dist/hqa.tar.gz".into(),
            links: [(".local/bin/hqa".into(), "hqa".into())]
                .into_iter()
                .collect(),
            verify: None,
        }],
    };
    std::fs::write(
        hqa.join("vm-tool.yaml"),
        serde_yaml_ng::to_string(&manifest).unwrap(),
    )
    .unwrap();
    git(&hqa, &["add", "vm-tool.yaml"]);
    git(&hqa, &["commit", "-m", "release"]);
    let hqa_commit = git_output(&hqa, &["rev-parse", "HEAD"]);
    let checkout_id = "checkout-build-sources";
    let submission_id = "submission-build-sources";
    let integration_root = data
        .join("agents")
        .join(checkout_id)
        .join("integrations")
        .join(submission_id);
    std::fs::create_dir_all(&integration_root).unwrap();
    let integration_bundle = integration_root.join("integration.bundle");
    git(
        &hqa,
        &[
            "bundle",
            "create",
            integration_bundle.to_str().unwrap(),
            "--all",
        ],
    );
    let submission = SubmissionRecord {
        submission_id: submission_id.into(),
        checkout_id: checkout_id.into(),
        package: "hqa".into(),
        branch: "main".into(),
        base_commit: hqa_commit.clone(),
        submitted_commit: hqa_commit.clone(),
        diff_digest: "a".repeat(64),
        state: WorkflowState::ReadyToRelease,
        validation: None,
        review: None,
        integration: Some(IntegrationRecord {
            canonical_commit: hqa_commit.clone(),
            integration_commit: hqa_commit,
            strategy: "workspace".into(),
            worktree: integration_root
                .join("source")
                .to_string_lossy()
                .into_owned(),
            validation: None,
            timestamp: chrono::Utc::now(),
        }),
        release_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let source = SourceManager::new(&data);
    let bundle = source
        .tool_build_source_bundle(&store, &submission, "hif")
        .await
        .unwrap();
    assert_eq!(
        bundle,
        source
            .tool_build_source_bundle(&store, &submission, "hif")
            .await
            .unwrap()
    );
    let clone = directory.path().join("build-source-clone");
    assert!(StdCommand::new("git")
        .arg("clone")
        .arg(&bundle)
        .arg(&clone)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success());
    git(&clone, &["checkout", "--detach", &hif_commit]);
    assert_eq!(
        std::fs::read_to_string(clone.join("source.txt")).unwrap(),
        "immutable input\n"
    );
    assert!(source
        .tool_build_source_bundle(&store, &submission, "other")
        .await
        .is_err());
    store
        .register_tool(RegisterTool {
            name: "hqa".into(),
            kind: ToolKind::Binary,
            repository: url::Url::from_file_path(&hqa).unwrap().into(),
            default_branch: "main".into(),
            build_sources: Vec::new(),
            workspace_release: false,
        })
        .await
        .unwrap();
    assert!(source
        .tool_build_source_bundle(&store, &submission, "hif")
        .await
        .is_err());
}

#[tokio::test]
async fn mirror_sync_removes_abandoned_clone_directories() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path().join("repository");
    std::fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "--initial-branch", "main"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    git(&repository, &["config", "user.name", "Test"]);
    std::fs::write(repository.join("README.md"), "test\n").unwrap();
    git(&repository, &["add", "README.md"]);
    git(&repository, &["commit", "-m", "initial"]);

    let sources = directory.path().join("sources");
    std::fs::create_dir(&sources).unwrap();
    let mirror = sources.join("package.git");
    let abandoned = temporary_mirror_path(&mirror, "abandoned");
    std::fs::create_dir(&abandoned).unwrap();
    std::fs::write(abandoned.join("partial"), "stale").unwrap();

    SourceManager::new(directory.path())
        .sync_mirror(&mirror, repository.to_str().unwrap())
        .await
        .unwrap();

    assert!(mirror.is_dir());
    assert!(!abandoned.exists());

    let abandoned_after_clone = temporary_mirror_path(&mirror, "after-clone");
    std::fs::create_dir(&abandoned_after_clone).unwrap();
    SourceManager::new(directory.path())
        .sync_mirror(&mirror, repository.to_str().unwrap())
        .await
        .unwrap();
    assert!(!abandoned_after_clone.exists());
}

#[tokio::test]
async fn tool_collection_checkout_uses_the_same_managed_source_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path().join("agent-skills");
    std::fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "--initial-branch", "main"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    git(&repository, &["config", "user.name", "Test"]);
    std::fs::write(
        repository.join("package.json"),
        r#"{"name":"agent-skills","version":"1.0.0"}"#,
    )
    .unwrap();
    git(&repository, &["add", "package.json"]);
    git(&repository, &["commit", "-m", "initial"]);

    let data = directory.path().join("data");
    let store = Store::open(&data).await.unwrap();
    store
        .register_tool(RegisterTool {
            name: "agent-skills".into(),
            kind: ToolKind::Collection,
            repository: url::Url::from_file_path(&repository).unwrap().into(),
            default_branch: "main".into(),
            build_sources: Vec::new(),
            workspace_release: false,
        })
        .await
        .unwrap();
    let checkout = store
        .create_checkout(CreateCheckout {
            package: "agent-skills".into(),
            agent: "codex".into(),
            consumers: vec!["project-a".into()],
            task: "update owner checklist".into(),
            workspace_release: false,
            source_only: false,
            lease_token: "lease-token-012345678901234567890123456789".into(),
            idempotency_key: "tool-checkout-1".into(),
        })
        .await
        .unwrap();
    let prepared = SourceManager::new(&data)
        .prepare(&store, &checkout)
        .await
        .unwrap();

    assert_eq!(prepared.source_kind, SourceKind::ToolCollection);
    assert_eq!(prepared.state, WorkflowState::CheckedOut);
    assert_eq!(prepared.base_branch.as_deref(), Some("main"));
    assert!(prepared
        .worktree
        .as_deref()
        .unwrap()
        .starts_with(data.join("agents").to_str().unwrap()));
}

#[tokio::test]
async fn canonical_workspace_bootstraps_and_integrates_without_remote_access_or_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    cargo_repository(
        &workspace,
        "[package]\nname='release-tool'\nversion='1.0.0'\n",
        "initial",
    );
    std::fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname='release-tool'\nversion='1.0.1'\n",
    )
    .unwrap();
    git(&workspace, &["add", "Cargo.toml"]);
    git(&workspace, &["commit", "-m", "release 1.0.1"]);
    let workspace_head = git_output(&workspace, &["rev-parse", "HEAD"]);
    let workspace_status = git_output(&workspace, &["status", "--porcelain"]);

    let data = directory.path().join("data");
    let store = Store::open(&data).await.unwrap();
    store
        .register_package(RegisterPackage {
            name: "release-tool".into(),
            ecosystem: PackageEcosystem::Cargo,
            repository: "https://unreachable.invalid/release-tool.git".into(),
            default_branch: "main".into(),
            workspace_release: true,
        })
        .await
        .unwrap();
    let checkout = store
        .create_checkout(CreateCheckout {
            package: "release-tool".into(),
            agent: "workspace-agent".into(),
            consumers: vec!["project-a".into()],
            task: "release committed canonical workspace".into(),
            workspace_release: true,
            source_only: false,
            lease_token: "lease-token-012345678901234567890123456789".into(),
            idempotency_key: "workspace-checkout".into(),
        })
        .await
        .unwrap()
        .checkout;
    assert_eq!(checkout.state, WorkflowState::Created);

    let bundle = directory.path().join("workspace.bundle");
    git(
        &workspace,
        &["bundle", "create", bundle.to_str().unwrap(), "HEAD"],
    );
    let source = SourceManager::new(&data);
    let submission = source
        .import_submission(&store, &checkout, &bundle)
        .await
        .unwrap();
    assert_eq!(submission.submitted_commit, workspace_head);
    assert_eq!(submission.state, WorkflowState::Submitted);
    let active_checkout = store.get_checkout(&checkout.checkout_id).await.unwrap();
    assert!(active_checkout.initial_release);
    assert_eq!(
        active_checkout.base_commit.as_deref(),
        Some(workspace_head.as_str())
    );
    let initial_diff = std::fs::read_to_string(
        data.join("agents")
            .join(&checkout.checkout_id)
            .join("submissions")
            .join(format!("{}.diff", &workspace_head[..16])),
    )
    .unwrap();
    assert!(initial_diff.contains("Cargo.toml"));
    assert!(initial_diff.contains("name='release-tool'"));
    store
        .validate_submission(
            &submission.submission_id,
            ValidationRequest {
                package: CheckOutcome::Passed,
                consumers: Default::default(),
                actor: "workspace-agent".into(),
                idempotency_key: "validate-workspace".into(),
            },
        )
        .await
        .unwrap();
    let approved = store
        .record_review(
            &submission.submission_id,
            ReviewRequest {
                decision: ReviewDecision::Approve,
                recommended_version: VersionRecommendation::Patch,
                api_diff: PublicApiDiff {
                    changed_paths: vec!["Cargo.toml".into()],
                    potentially_breaking: false,
                },
                reason: "workspace release is valid".into(),
                required_followups: Vec::new(),
                merge_strategy: "rebase".into(),
                reviewer: "reviewer".into(),
                idempotency_key: "review-workspace".into(),
            },
        )
        .await
        .unwrap();
    let integrated = source
        .prepare_integration(
            &store,
            &approved,
            IntegrationRequest {
                actor: "workspace-agent".into(),
                strategy: "rebase".into(),
                idempotency_key: "integrate-workspace".into(),
            },
        )
        .await
        .unwrap();
    let integration = integrated.integration.unwrap();
    assert_eq!(integration.integration_commit, workspace_head);
    assert_eq!(integration.strategy, "workspace");
    assert!(source
        .integration_bundle(&store.submission(&submission.submission_id).await.unwrap())
        .unwrap()
        .is_file());
    assert_eq!(
        git_output(&workspace, &["rev-parse", "HEAD"]),
        workspace_head
    );
    assert_eq!(
        git_output(&workspace, &["status", "--porcelain"]),
        workspace_status
    );
}

#[tokio::test]
async fn later_workspace_release_uses_the_last_published_commit_across_all_new_commits() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    cargo_repository(
        &workspace,
        "[package]\nname='release-tool'\nversion='1.0.0'\n",
        "published baseline",
    );
    let published_commit = git_output(&workspace, &["rev-parse", "HEAD"]);
    std::fs::create_dir(workspace.join("src")).unwrap();
    std::fs::write(workspace.join("src/lib.rs"), "pub fn added() {}\n").unwrap();
    git(&workspace, &["add", "src/lib.rs"]);
    git(&workspace, &["commit", "-m", "add public API"]);
    std::fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname='release-tool'\nversion='1.1.0'\n",
    )
    .unwrap();
    git(&workspace, &["add", "Cargo.toml"]);
    git(&workspace, &["commit", "-m", "prepare release"]);
    let submitted_commit = git_output(&workspace, &["rev-parse", "HEAD"]);

    let data = directory.path().join("data");
    let store = Store::open(&data).await.unwrap();
    store
        .register_package(RegisterPackage {
            name: "release-tool".into(),
            ecosystem: PackageEcosystem::Cargo,
            repository: "https://unreachable.invalid/release-tool.git".into(),
            default_branch: "main".into(),
            workspace_release: true,
        })
        .await
        .unwrap();
    let now = chrono::Utc::now();
    store.database.lock().await.releases.insert(
        "rel-published-baseline".into(),
        ReleaseRecord {
            release_id: "rel-published-baseline".into(),
            submission_id: "sub-published-baseline".into(),
            checkout_id: "checkout-published-baseline".into(),
            package: "release-tool".into(),
            version: "1.0.0".into(),
            source_repository: "https://unreachable.invalid/release-tool.git".into(),
            source_commit: published_commit.clone(),
            tag: "v1.0.0".into(),
            artifact_digest: "a".repeat(64),
            source_pushed: false,
            source_archive_digest: Some("b".repeat(64)),
            registry: "http://gateway:8080/cargo/".into(),
            expected_publications: Vec::new(),
            publications: Vec::new(),
            state: WorkflowState::Published,
            created_at: now,
            updated_at: now,
        },
    );
    let checkout = store
        .create_checkout(CreateCheckout {
            package: "release-tool".into(),
            agent: "workspace-agent".into(),
            consumers: Vec::new(),
            task: "release all committed workspace changes".into(),
            workspace_release: true,
            source_only: false,
            lease_token: "lease-token-012345678901234567890123456789".into(),
            idempotency_key: "workspace-checkout-published-baseline".into(),
        })
        .await
        .unwrap()
        .checkout;
    let bundle = directory.path().join("workspace-published.bundle");
    git(
        &workspace,
        &["bundle", "create", bundle.to_str().unwrap(), "HEAD"],
    );
    let submission = SourceManager::new(&data)
        .import_submission(&store, &checkout, &bundle)
        .await
        .unwrap();
    let active_checkout = store.get_checkout(&checkout.checkout_id).await.unwrap();

    assert!(!active_checkout.initial_release);
    assert_eq!(submission.base_commit, published_commit);
    assert_eq!(submission.submitted_commit, submitted_commit);
    let diff = std::fs::read_to_string(
        data.join("agents")
            .join(&checkout.checkout_id)
            .join("submissions")
            .join(format!("{}.diff", &submitted_commit[..16])),
    )
    .unwrap();
    assert!(diff.contains("src/lib.rs"));
    assert!(diff.contains("version='1.1.0'"));
}

fn cargo_repository(repository: &Path, manifest: &str, message: &str) {
    std::fs::create_dir(repository).unwrap();
    git(repository, &["init", "--initial-branch", "main"]);
    git(repository, &["config", "user.email", "test@example.com"]);
    git(repository, &["config", "user.name", "Test"]);
    std::fs::write(repository.join("Cargo.toml"), manifest).unwrap();
    git(repository, &["add", "Cargo.toml"]);
    git(repository, &["commit", "-m", message]);
}

#[tokio::test]
async fn initial_managed_checkout_submits_its_canonical_head_without_an_empty_commit() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path().join("repository");
    cargo_repository(
        &repository,
        "[package]\nname='first-release'\nversion='1.0.0'\n",
        "prepare first release",
    );
    let canonical_head = git_output(&repository, &["rev-parse", "HEAD"]);
    let data = directory.path().join("data");
    let store = Store::open(&data).await.unwrap();
    store
        .register_package(RegisterPackage {
            name: "first-release".into(),
            ecosystem: PackageEcosystem::Cargo,
            repository: url::Url::from_file_path(&repository).unwrap().into(),
            default_branch: "main".into(),
            workspace_release: false,
        })
        .await
        .unwrap();
    let lease = store
        .create_checkout(CreateCheckout {
            package: "first-release".into(),
            agent: "agent-1".into(),
            consumers: Vec::new(),
            task: "publish the existing first release".into(),
            workspace_release: false,
            source_only: true,
            lease_token: "lease-token-012345678901234567890123456789".into(),
            idempotency_key: "initial-managed-checkout".into(),
        })
        .await
        .unwrap();
    let source = SourceManager::new(&data);
    let prepared = source.prepare(&store, &lease).await.unwrap();
    assert!(prepared.initial_release);
    let checkout_bundle = source.archive(&prepared).await.unwrap();
    let consumer = directory.path().join("consumer");
    git(
        directory.path(),
        &[
            "clone",
            checkout_bundle.to_str().unwrap(),
            consumer.to_str().unwrap(),
        ],
    );
    git(&consumer, &["switch", prepared.branch.as_deref().unwrap()]);
    let active = store
        .transition(
            &prepared.checkout_id,
            vm_packages::TransitionRequest {
                next: WorkflowState::Active,
                actor: "agent-1".into(),
                reason: "consumer attached".into(),
                commit: prepared.base_commit.clone(),
                validation_result: None,
                idempotency_key: "initial-managed-active".into(),
            },
        )
        .await
        .unwrap();
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
    let submission = source
        .import_submission(&store, &active, &submitted_bundle)
        .await
        .unwrap();
    assert_eq!(submission.base_commit, canonical_head);
    assert_eq!(submission.submitted_commit, canonical_head);
    let diff = std::fs::read_to_string(
        data.join("agents")
            .join(&prepared.checkout_id)
            .join("submissions")
            .join(format!("{}.diff", &canonical_head[..16])),
    )
    .unwrap();
    assert!(diff.contains("Cargo.toml"));
    store
        .validate_submission(
            &submission.submission_id,
            ValidationRequest {
                package: CheckOutcome::Passed,
                consumers: Default::default(),
                actor: "agent-1".into(),
                idempotency_key: "validate-initial-managed".into(),
            },
        )
        .await
        .unwrap();
    let approved = store
        .record_review(
            &submission.submission_id,
            ReviewRequest {
                decision: ReviewDecision::Approve,
                recommended_version: VersionRecommendation::Patch,
                api_diff: PublicApiDiff {
                    changed_paths: vec!["Cargo.toml".into()],
                    potentially_breaking: false,
                },
                reason: "initial managed release is valid".into(),
                required_followups: Vec::new(),
                merge_strategy: "rebase".into(),
                reviewer: "reviewer".into(),
                idempotency_key: "review-initial-managed".into(),
            },
        )
        .await
        .unwrap();
    let integrated = source
        .prepare_integration(
            &store,
            &approved,
            IntegrationRequest {
                actor: "agent-1".into(),
                strategy: "rebase".into(),
                idempotency_key: "integrate-initial-managed".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        integrated.integration.unwrap().integration_commit,
        canonical_head
    );
}

#[tokio::test]
async fn package_checkout_lifecycle_stays_inside_managed_agent_storage() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path().join("repository");
    cargo_repository(
        &repository,
        "[package]\nname='auth'\nversion='1.0.0'\n",
        "initial",
    );

    let data = directory.path().join("data");
    let store = Store::open(&data).await.unwrap();
    store
        .register_package(RegisterPackage {
            name: "auth".into(),
            ecosystem: PackageEcosystem::Cargo,
            repository: url::Url::from_file_path(&repository).unwrap().into(),
            default_branch: "main".into(),
            workspace_release: false,
        })
        .await
        .unwrap();
    let checkout = store
        .create_checkout(CreateCheckout {
            package: "auth".into(),
            agent: "agent-1".into(),
            consumers: vec!["project-a".into()],
            task: "change auth".into(),
            workspace_release: false,
            source_only: false,
            lease_token: "lease-token-012345678901234567890123456789".into(),
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
    assert!(!data
        .join("agents")
        .join(&prepared.checkout_id)
        .join("source")
        .exists());
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
    assert!(!data
        .join("agents")
        .join(&active.checkout_id)
        .join("source")
        .exists());
    assert!(source.submission_bundle(&submission).unwrap().is_file());

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
    let retained_digest = source.retain_release_source(&submission).await.unwrap();
    let retained = data
        .join("agents/releases")
        .join(&submission.submission_id)
        .join(format!("{retained_digest}.bundle"));
    assert!(retained.is_file());
    assert!(std::fs::metadata(&retained)
        .unwrap()
        .permissions()
        .readonly());
    assert!(repository.join(".git").is_dir());
    assert!(repository.join("Cargo.toml").is_file());
    assert!(data.join("sources").is_dir());

    source.restore_checkout(&store, &active).await.unwrap();
    source.restore_checkout(&store, &active).await.unwrap();
    assert!(data
        .join("agents")
        .join(&active.checkout_id)
        .join("source")
        .is_dir());
    let restored_source = data.join("agents").join(&active.checkout_id).join("source");
    assert_eq!(
        git_output(&restored_source, &["rev-parse", "HEAD"]),
        submission.submitted_commit
    );
    assert!(source.archive(&active).await.unwrap().is_file());

    source.cleanup_checkout(&active).await.unwrap();
    source.cleanup_checkout(&active).await.unwrap();
    assert!(!data.join("agents").join(&active.checkout_id).exists());
    assert!(retained.is_file());
    assert!(repository.join(".git").is_dir());
    assert!(repository.join("Cargo.toml").is_file());
    assert!(data.join("sources").is_dir());
}

#[tokio::test]
async fn consumer_rollout_isolated_bundle_pushes_only_its_upgrade_branch() {
    let directory = tempfile::tempdir().unwrap();
    let package_repository = directory.path().join("package");
    cargo_repository(
        &package_repository,
        "[package]\nname='auth'\nversion='1.1.0'\n",
        "auth release",
    );

    let consumer_repository = directory.path().join("consumer-repository");
    cargo_repository(
        &consumer_repository,
        "[package]\nname='app'\nversion='1.0.0'\n[dependencies]\nauth='1.0.0'\n",
        "initial consumer",
    );

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
            workspace_release: false,
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
            source_archive_digest: None,
            registry: "https://packages.example/cargo/".into(),
            expected_publications: Vec::new(),
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
