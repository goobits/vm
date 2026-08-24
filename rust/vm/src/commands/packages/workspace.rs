use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vm_packages::{
    repository_urls_equivalent, CheckoutRecord, CleanupRequest, CreateCheckout, PackageDefinition,
    ReleaseRecord, SourceKind, ToolDefinition, ToolKind, TransitionRequest, WorkflowState,
};

use crate::error::{VmError, VmResult};

use super::{
    checkout,
    discovery::{normalize_repository_url, package_name, tool_manifest},
    runtime::{checkout_root, copy_private, exec_output, write_checkout_access, GuestRuntime},
};

const STATE_SCHEMA: u32 = 1;

pub(super) struct WorkspaceRelease {
    pub(super) checkout_id: String,
    pub(super) source: String,
    state_path: PathBuf,
    state: WorkspaceReleaseState,
}

pub(super) enum WorkspacePreparation {
    Published {
        release: vm_packages::ReleaseRecord,
        source_kind: SourceKind,
    },
    Pending(WorkspaceRelease),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceReleaseState {
    schema: u32,
    source: String,
    repository: String,
    lease_token: String,
    idempotency_key: String,
    checkout_id: Option<String>,
    source_commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegisteredSource {
    name: String,
    kind: SourceKind,
}

impl WorkspaceRelease {
    pub(super) fn record_commit(&mut self, subject: &GuestRuntime, commit: &str) -> VmResult<()> {
        self.state.source_commit = commit.to_string();
        self.save(subject)
    }

    fn save(&self, subject: &GuestRuntime) -> VmResult<()> {
        save_state(subject, &self.state_path, &self.state)
    }
}

pub(super) async fn prepare(subject: &GuestRuntime) -> VmResult<WorkspacePreparation> {
    let source = git_root(subject)?;
    ensure_clean(subject, &source)?;
    let head = git_output(subject, &source, ["rev-parse", "--verify", "HEAD^{commit}"])?;
    let origin = git_output(subject, &source, ["remote", "get-url", "origin"])?;
    let repository = normalize_repository_url(origin.trim())?;
    let client = subject.client()?;
    let (packages, mut tools) = tokio::try_join!(client.package_definitions(), client.tools())?;
    if packages
        .iter()
        .all(|package| !repository_urls_equivalent(&package.repository, &repository))
        && tools
            .iter()
            .all(|tool| !repository_urls_equivalent(&tool.repository, &repository))
        && Path::new(&source).join("vm-tool.yaml").is_file()
    {
        tool_manifest(Path::new(&source))?;
        tools.push(client.register_attested_tool().await?);
    }
    let registered = resolve_registered_source(&source, &repository, &packages, &tools)?;
    let key = format!(
        "workspace-release-{}",
        &vm_packages::sha256_hex(format!(
            "{}\0{}\0{}",
            subject.consumer(),
            registered.name,
            repository
        ))[..32]
    );
    let state_path = subject.request_state_path(&key)?;
    let saved_state =
        load_state(&state_path)?.filter(|state| state_matches(state, &registered, &repository));
    if let Some(release) = matching_release(client.releases().await?, &registered.name, &head) {
        if let Some(state) = saved_state.as_ref() {
            cleanup_redundant_checkout(subject, &client, &registered, &release, state, &state_path)
                .await?;
        }
        return Ok(WorkspacePreparation::Published {
            release,
            source_kind: registered.kind,
        });
    }
    let mut state = saved_state.unwrap_or_else(|| new_state(&registered.name, &repository, &head));

    let existing = match state.checkout_id.as_deref() {
        Some(checkout_id) => client.checkout(checkout_id).await.ok(),
        None => None,
    };
    let checkout = match existing {
        Some(checkout)
            if matches!(
                checkout.state,
                WorkflowState::Published | WorkflowState::Closed
            ) && state.source_commit == head =>
        {
            checkout
        }
        Some(checkout) if !checkout.state.revokes_lease() => checkout,
        Some(_) => {
            state = new_state(&registered.name, &repository, &head);
            save_state(subject, &state_path, &state)?;
            create_checkout(subject, &registered, &mut state, &state_path).await?
        }
        None => {
            save_state(subject, &state_path, &state)?;
            create_checkout(subject, &registered, &mut state, &state_path).await?
        }
    };
    validate_checkout(subject, &checkout, &registered)?;
    let root = checkout_root(subject, &checkout.checkout_id)?;
    write_checkout_access(subject, &root, &state.lease_token)?;
    Ok(WorkspacePreparation::Pending(WorkspaceRelease {
        checkout_id: checkout.checkout_id,
        source,
        state_path,
        state,
    }))
}

async fn cleanup_redundant_checkout(
    subject: &GuestRuntime,
    client: &vm_packages::PackageInfrastructureClient,
    registered: &RegisteredSource,
    release: &ReleaseRecord,
    state: &WorkspaceReleaseState,
    state_path: &Path,
) -> VmResult<()> {
    let Some(checkout_id) = state.checkout_id.as_deref() else {
        remove_state(state_path)?;
        return Ok(());
    };
    let checkout = client.checkout(checkout_id).await?;
    if !redundant_checkout_matches(subject.consumer(), registered, release, state, &checkout) {
        return Ok(());
    }
    let terminal = if checkout.state.revokes_lease() {
        checkout
    } else {
        client
            .transition(
                checkout_id,
                &TransitionRequest {
                    next: WorkflowState::Cancelled,
                    actor: checkout.agent.clone(),
                    reason: "canonical workspace commit is already published".into(),
                    commit: None,
                    validation_result: Some("published_source_noop".into()),
                    idempotency_key: format!("cancel-published-{checkout_id}"),
                },
            )
            .await?
    };
    checkout::cleanup_guest(subject, &terminal)?;
    client
        .cleanup_checkout(
            checkout_id,
            &CleanupRequest {
                actor: terminal.agent,
                idempotency_key: format!("guest-cleanup-published-{checkout_id}"),
            },
        )
        .await?;
    remove_state(state_path)
}

fn redundant_checkout_matches(
    consumer: &str,
    registered: &RegisteredSource,
    release: &ReleaseRecord,
    state: &WorkspaceReleaseState,
    checkout: &CheckoutRecord,
) -> bool {
    state.source_commit.trim() == release.source_commit
        && checkout.workspace_release
        && checkout.package == registered.name
        && checkout.source_kind == registered.kind
        && checkout.base_commit.as_deref() == Some(release.source_commit.as_str())
        && checkout
            .consumers
            .iter()
            .any(|candidate| candidate == consumer)
}

fn remove_state(path: &Path) -> VmResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(VmError::filesystem(
            error,
            path.display().to_string(),
            "remove completed workspace release state",
        )),
    }
}

fn matching_release(
    releases: Vec<vm_packages::ReleaseRecord>,
    source: &str,
    head: &str,
) -> Option<vm_packages::ReleaseRecord> {
    let head = head.trim();
    releases
        .into_iter()
        .filter(|release| {
            release.package == source
                && release.source_commit == head
                && release.state == WorkflowState::Published
        })
        .max_by_key(|release| release.created_at)
}

async fn create_checkout(
    subject: &GuestRuntime,
    registered: &RegisteredSource,
    state: &mut WorkspaceReleaseState,
    state_path: &Path,
) -> VmResult<CheckoutRecord> {
    let created = subject
        .client()?
        .create_checkout(&CreateCheckout {
            package: registered.name.clone(),
            agent: "workspace-agent".into(),
            consumers: vec![subject.consumer().to_string()],
            task: "release committed canonical workspace".into(),
            workspace_release: true,
            source_only: false,
            lease_token: state.lease_token.clone(),
            idempotency_key: state.idempotency_key.clone(),
        })
        .await?;
    state.checkout_id = Some(created.checkout.checkout_id.clone());
    save_state(subject, state_path, state)?;
    Ok(created.checkout)
}

fn resolve_registered_source(
    root: &str,
    repository: &str,
    packages: &[PackageDefinition],
    tools: &[ToolDefinition],
) -> VmResult<RegisteredSource> {
    let packages = packages
        .iter()
        .filter(|package| repository_urls_equivalent(&package.repository, repository))
        .collect::<Vec<_>>();
    let tools = tools
        .iter()
        .filter(|tool| repository_urls_equivalent(&tool.repository, repository))
        .collect::<Vec<_>>();
    if packages.len() + tools.len() != 1 {
        let message = if packages.is_empty() && tools.is_empty() {
            "Workspace Git origin is not registered in the package catalog".to_string()
        } else {
            "Workspace Git origin is ambiguous in the package catalog".to_string()
        };
        return Err(VmError::validation(
            message,
            Some("Run `vm packages doctor --fix` on the controller host"),
        ));
    }
    if let Some(package) = packages.first() {
        if !package.workspace_release {
            return Err(unattested_workspace());
        }
        let actual = package_name(Path::new(root), package.ecosystem)?;
        if actual != package.name {
            return Err(VmError::validation(
                format!(
                    "Workspace package identity '{actual}' does not match registered source '{}'",
                    package.name
                ),
                Some("Run `vm packages doctor --fix` on the controller host"),
            ));
        }
        return Ok(RegisteredSource {
            name: package.name.clone(),
            kind: SourceKind::Package,
        });
    }
    let tool = tools[0];
    if !tool.workspace_release {
        return Err(unattested_workspace());
    }
    let manifest = tool_manifest(Path::new(root))?;
    if manifest.kind != tool.kind {
        return Err(VmError::validation(
            "Workspace tool kind does not match its registered catalog identity",
            Some("Run `vm packages doctor --fix` on the controller host"),
        ));
    }
    Ok(RegisteredSource {
        name: tool.name.clone(),
        kind: match tool.kind {
            ToolKind::Binary => SourceKind::ToolBinary,
            ToolKind::Collection => SourceKind::ToolCollection,
        },
    })
}

fn unattested_workspace() -> VmError {
    VmError::validation(
        "Workspace source is not registered as a read-only canonical workspace",
        Some("Run `vm packages register <local-path>` on the controller host"),
    )
}

fn validate_checkout(
    subject: &GuestRuntime,
    checkout: &CheckoutRecord,
    registered: &RegisteredSource,
) -> VmResult<()> {
    if !checkout.workspace_release
        || checkout.package != registered.name
        || checkout.source_kind != registered.kind
        || checkout.consumers != [subject.consumer()]
    {
        return Err(VmError::validation(
            "Workspace release state does not match the registered source and environment",
            Some("Run `vm packages doctor --fix`"),
        ));
    }
    Ok(())
}

fn git_root(subject: &GuestRuntime) -> VmResult<String> {
    let root = exec_output(subject, ["git", "rev-parse", "--show-toplevel"])?;
    let root = std::fs::canonicalize(root.trim()).map_err(|error| {
        VmError::filesystem(error, root.trim(), "resolve canonical workspace Git root")
    })?;
    let canonical_workspace = subject.canonical_workspace()?;
    let expected = std::fs::canonicalize(canonical_workspace).map_err(|error| {
        VmError::filesystem(
            error,
            canonical_workspace.display().to_string(),
            "resolve configured project workspace",
        )
    })?;
    validate_workspace_root(&root, &expected)?;
    Ok(root.to_string_lossy().into_owned())
}

fn validate_workspace_root(root: &Path, expected: &Path) -> VmResult<()> {
    if root != expected {
        return Err(VmError::validation(
            format!(
                "Repository {} is not the configured canonical workspace {}",
                root.display(),
                expected.display()
            ),
            Some("Run `vm packages checkout <source>` inside a managed VM"),
        ));
    }
    Ok(())
}

fn git_output<const N: usize>(
    subject: &GuestRuntime,
    source: &str,
    arguments: [&str; N],
) -> VmResult<String> {
    let mut command = vec!["git".to_string(), "-C".into(), source.to_string()];
    command.extend(arguments.into_iter().map(str::to_string));
    exec_output(subject, command)
}

fn ensure_clean(subject: &GuestRuntime, source: &str) -> VmResult<()> {
    let status = git_output(
        subject,
        source,
        ["status", "--porcelain", "--untracked-files=no"],
    )?;
    if status.trim().is_empty() {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("Canonical workspace has uncommitted tracked changes:\n{status}"),
            Some("Commit the listed tracked changes, then run `vm packages release`"),
        ))
    }
}

fn new_state(source: &str, repository: &str, head: &str) -> WorkspaceReleaseState {
    WorkspaceReleaseState {
        schema: STATE_SCHEMA,
        source: source.to_string(),
        repository: repository.to_string(),
        lease_token: vm_core::secrets::generate_random_password(48),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        checkout_id: None,
        source_commit: head.to_string(),
    }
}

fn load_state(path: &Path) -> VmResult<Option<WorkspaceReleaseState>> {
    match std::fs::read(path) {
        Ok(content) => Ok(serde_json::from_slice(&content).ok()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(VmError::from(error)),
    }
}

fn state_matches(
    state: &WorkspaceReleaseState,
    registered: &RegisteredSource,
    repository: &str,
) -> bool {
    state.schema == STATE_SCHEMA
        && state.source == registered.name
        && state.repository == repository
        && (32..=256).contains(&state.lease_token.len())
}

fn save_state(subject: &GuestRuntime, path: &Path, state: &WorkspaceReleaseState) -> VmResult<()> {
    let content = serde_json::to_vec(state).map_err(VmError::from)?;
    copy_private(subject, &content, &path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use vm_packages::{PackageEcosystem, ReleaseRecord, ToolSourceManifest};

    use super::*;

    fn release(package: &str, commit: &str, state: WorkflowState) -> ReleaseRecord {
        let now = Utc::now();
        ReleaseRecord {
            release_id: format!("release-{package}"),
            submission_id: "submission-1".into(),
            checkout_id: "checkout-1".into(),
            package: package.into(),
            version: "1.2.0".into(),
            source_repository: "ssh://git@example.com/example/tool.git".into(),
            source_commit: commit.into(),
            tag: "v1.2.0".into(),
            artifact_digest: "a".repeat(64),
            source_pushed: true,
            source_archive_digest: None,
            registry: "http://gateway/tools/example".into(),
            expected_publications: Vec::new(),
            publications: Vec::new(),
            state,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn published_workspace_head_resolves_existing_release() {
        let head = "a".repeat(40);
        let matched = matching_release(
            vec![
                release("typemill", &head, WorkflowState::Published),
                release("typemill", &"b".repeat(40), WorkflowState::Published),
                release("other", &head, WorkflowState::Published),
            ],
            "typemill",
            &format!("{head}\n"),
        )
        .unwrap();

        assert_eq!(matched.package, "typemill");
        assert_eq!(matched.source_commit, head);
    }

    #[test]
    fn published_source_matches_only_its_redundant_workspace_checkout() {
        let head = "a".repeat(40);
        let release = release("typemill", &head, WorkflowState::Published);
        let registered = RegisteredSource {
            name: "typemill".into(),
            kind: SourceKind::ToolBinary,
        };
        let state = WorkspaceReleaseState {
            schema: STATE_SCHEMA,
            source: "typemill".into(),
            repository: "ssh://git@example.com/example/tool.git".into(),
            lease_token: "x".repeat(48),
            idempotency_key: "workspace-release".into(),
            checkout_id: Some("checkout-1".into()),
            source_commit: format!("{head}\n"),
        };
        let now = Utc::now();
        let checkout = CheckoutRecord {
            checkout_id: "checkout-1".into(),
            package: "typemill".into(),
            source_kind: SourceKind::ToolBinary,
            agent: "typemill".into(),
            consumers: vec!["typemill".into()],
            task: "release canonical workspace".into(),
            workspace_release: true,
            source_only: false,
            initial_release: false,
            state: WorkflowState::Active,
            base_branch: Some("main".into()),
            base_commit: Some(head.clone()),
            branch: Some("workspace/checkout-1".into()),
            worktree: Some("/data/agents/checkout-1/source".into()),
            lease: None,
            created_at: now,
            updated_at: now,
            transitions: Vec::new(),
        };

        assert!(redundant_checkout_matches(
            "typemill",
            &registered,
            &release,
            &state,
            &checkout
        ));
        assert!(!redundant_checkout_matches(
            "another-environment",
            &registered,
            &release,
            &state,
            &checkout
        ));
    }

    #[test]
    fn registered_workspace_requires_canonical_remote_identity_and_attestation() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"name":"shared-auth"}"#,
        )
        .unwrap();
        let packages = vec![PackageDefinition {
            name: "shared-auth".into(),
            ecosystem: PackageEcosystem::Npm,
            repository: "ssh://git@example.com/shared/auth.git".into(),
            default_branch: "main".into(),
            workspace_release: true,
            registered_at: Utc::now(),
        }];
        let source = resolve_registered_source(
            directory.path().to_str().unwrap(),
            "ssh://git@example.com/shared/auth.git",
            &packages,
            &[],
        )
        .unwrap();
        assert_eq!(source.name, "shared-auth");
        assert_eq!(source.kind, SourceKind::Package);

        let github_packages = vec![PackageDefinition {
            repository: "ssh://git@github.com/goobits/shared-auth.git".into(),
            ..packages[0].clone()
        }];
        assert!(resolve_registered_source(
            directory.path().to_str().unwrap(),
            "https://github.com/goobits/shared-auth.git",
            &github_packages,
            &[],
        )
        .is_ok());

        let mut unattested = packages;
        unattested[0].workspace_release = false;
        assert!(resolve_registered_source(
            directory.path().to_str().unwrap(),
            "ssh://git@example.com/shared/auth.git",
            &unattested,
            &[],
        )
        .is_err());
    }

    #[test]
    fn binary_tool_identity_comes_from_catalog_remote_not_mount_name() {
        let directory = tempfile::tempdir().unwrap();
        let manifest = ToolSourceManifest {
            schema: 1,
            kind: ToolKind::Binary,
            version: Some("1.0.0".into()),
            build_sources: Vec::new(),
            builds: vec![vm_packages::ToolBuild {
                target: "linux-amd64".into(),
                command: vec!["make".into()],
                archive: "dist/tool-linux-amd64.tar.gz".into(),
                links: [(".local/bin/tool".into(), "bin/tool".into())]
                    .into_iter()
                    .collect(),
                verify: Some(vec!["bin/tool".into(), "--version".into()]),
            }],
        };
        std::fs::write(
            directory.path().join("vm-tool.yaml"),
            serde_yaml_ng::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let tools = vec![ToolDefinition {
            name: "release-tool".into(),
            kind: ToolKind::Binary,
            repository: "https://example.com/tools/release-tool.git".into(),
            default_branch: "main".into(),
            build_sources: Vec::new(),
            workspace_release: true,
            registered_at: Utc::now(),
        }];
        let source = resolve_registered_source(
            directory.path().to_str().unwrap(),
            "https://example.com/tools/release-tool.git",
            &[],
            &tools,
        )
        .unwrap();
        assert_eq!(source.name, "release-tool");
        assert_eq!(source.kind, SourceKind::ToolBinary);
    }

    #[test]
    fn canonical_release_rejects_a_second_clone_of_the_registered_source() {
        let configured = Path::new("/workspace");
        assert!(validate_workspace_root(configured, configured).is_ok());

        let error = validate_workspace_root(Path::new("/tmp/clone"), configured).unwrap_err();
        assert!(error
            .to_string()
            .contains("not the configured canonical workspace"));
        assert!(error.hint().unwrap().contains("vm packages checkout"));
    }
}
