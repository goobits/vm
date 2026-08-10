use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::sync::Mutex;
use vm_packages::{CheckoutLease, CheckoutRecord, TransitionRequest, WorkflowState};
use vm_packages::{IntegrationRecord, IntegrationRequest, SubmissionRecord};

use crate::{io::atomic_write, ImportedSubmission, Store, WorkError, WorkResult};

#[derive(Clone)]
pub struct SourceManager {
    root: PathBuf,
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl SourceManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn prepare(
        &self,
        store: &Store,
        checkout: &CheckoutLease,
    ) -> WorkResult<CheckoutRecord> {
        if checkout.checkout.state != WorkflowState::Created {
            return Ok(checkout.checkout.clone());
        }
        let definition = store.package(&checkout.checkout.package).await?;
        let lock = self.lock(&format!("package:{}", definition.name)).await;
        let _guard = lock.lock().await;
        let result = self
            .prepare_locked(store, &checkout.checkout, &definition)
            .await;
        if let Err(error) = &result {
            let _ = store
                .transition(
                    &checkout.checkout.checkout_id,
                    TransitionRequest {
                        next: WorkflowState::Failed,
                        actor: "package-controller".into(),
                        reason: format!("source checkout failed: {error}"),
                        commit: None,
                        validation_result: Some("failed".into()),
                        idempotency_key: format!("source-failed-{}", checkout.checkout.checkout_id),
                    },
                )
                .await;
        }
        result
    }

    pub async fn archive(&self, checkout: &CheckoutRecord) -> WorkResult<PathBuf> {
        let lock = self
            .lock(&format!("checkout:{}", checkout.checkout_id))
            .await;
        let _guard = lock.lock().await;
        let source = checkout
            .worktree
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| WorkError::Conflict("checkout source is not ready".into()))?;
        let expected = self
            .root
            .join("agents")
            .join(&checkout.checkout_id)
            .join("source");
        if source != expected || !source.is_dir() {
            return Err(WorkError::Internal(
                "checkout source escaped its managed directory".into(),
            ));
        }
        let archive = self
            .root
            .join("agents")
            .join(&checkout.checkout_id)
            .join("source.bundle");
        if tokio::fs::try_exists(&archive).await? {
            tokio::fs::remove_file(&archive).await?;
        }
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("bundle")
                .arg("create")
                .arg(&archive)
                .arg("--all"),
            "bundle isolated package checkout",
        )
        .await?;
        Ok(archive)
    }

    pub async fn submission_staging_path(&self, checkout: &CheckoutRecord) -> WorkResult<PathBuf> {
        let source = self.checkout_source(checkout)?;
        if !source.is_dir() {
            return Err(WorkError::Conflict("checkout source is not ready".into()));
        }
        let uploads = source
            .parent()
            .expect("managed source has a parent")
            .join("uploads");
        tokio::fs::create_dir_all(&uploads).await?;
        Ok(uploads.join(format!(
            "{}.bundle",
            vm_core::secrets::generate_random_password(16)
        )))
    }

    pub async fn import_submission(
        &self,
        store: &Store,
        checkout: &CheckoutRecord,
        bundle: &Path,
    ) -> WorkResult<vm_packages::SubmissionRecord> {
        let lock = self
            .lock(&format!("checkout:{}", checkout.checkout_id))
            .await;
        let _guard = lock.lock().await;
        let source = self.checkout_source(checkout)?;
        let branch = checkout
            .branch
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("checkout branch is missing".into()))?;
        let base_commit = checkout
            .base_commit
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("checkout base commit is missing".into()))?;
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("bundle")
                .arg("verify")
                .arg(bundle),
            "verify submitted Git bundle",
        )
        .await?;
        let heads = git_output(
            self.git()
                .arg("bundle")
                .arg("list-heads")
                .arg(bundle)
                .arg(format!("refs/heads/{branch}")),
            "read submitted Git bundle head",
        )
        .await?;
        let submitted_commit = heads
            .split_whitespace()
            .next()
            .filter(|commit| {
                matches!(commit.len(), 40 | 64)
                    && commit
                        .chars()
                        .all(|character| character.is_ascii_hexdigit())
            })
            .ok_or_else(|| {
                WorkError::Invalid("bundle does not contain the checkout branch".into())
            })?
            .to_string();
        if submitted_commit == base_commit {
            return Err(WorkError::Invalid(
                "submission contains no commits beyond its base".into(),
            ));
        }
        let submission_ref = format!(
            "refs/submissions/{}",
            submitted_commit.chars().take(16).collect::<String>()
        );
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("fetch")
                .arg(bundle)
                .arg(format!("refs/heads/{branch}:{submission_ref}")),
            "import submitted package commits",
        )
        .await?;
        if run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("merge-base")
                .arg("--is-ancestor")
                .arg(base_commit)
                .arg(&submitted_commit),
            "verify submitted commit ancestry",
        )
        .await
        .is_err()
        {
            return Err(WorkError::Conflict(
                "submitted history does not descend from the recorded base commit".into(),
            ));
        }
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("diff")
                .arg("--check")
                .arg(format!("{base_commit}..{submitted_commit}")),
            "validate submitted diff",
        )
        .await?;
        let diff = run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("diff")
                .arg("--binary")
                .arg(format!("{base_commit}..{submitted_commit}")),
            "capture submitted diff",
        )
        .await?
        .stdout;
        let submissions = source
            .parent()
            .expect("managed source has a parent")
            .join("submissions");
        tokio::fs::create_dir_all(&submissions).await?;
        let key = submitted_commit.chars().take(16).collect::<String>();
        atomic_write(submissions.join(format!("{key}.diff")), diff.clone()).await?;
        let durable_bundle = submissions.join(format!("{key}.bundle"));
        if !tokio::fs::try_exists(&durable_bundle).await? {
            tokio::fs::rename(bundle, &durable_bundle).await?;
        } else {
            tokio::fs::remove_file(bundle).await?;
        }
        store
            .record_submission(
                &checkout.checkout_id,
                ImportedSubmission {
                    submitted_commit,
                    diff_digest: digest_hex(&diff),
                },
            )
            .await
    }

    pub async fn prepare_integration(
        &self,
        store: &Store,
        submission: &SubmissionRecord,
        request: IntegrationRequest,
    ) -> WorkResult<SubmissionRecord> {
        if submission.state != WorkflowState::Approved {
            return Err(WorkError::Conflict(
                "only an approved submission can be integrated".into(),
            ));
        }
        if !matches!(request.strategy.as_str(), "rebase" | "merge") {
            return Err(WorkError::Invalid(
                "integration strategy must be rebase or merge".into(),
            ));
        }
        let definition = store.package(&submission.package).await?;
        let checkout = store.get_checkout(&submission.checkout_id).await?;
        let lock = self.lock(&format!("package:{}", definition.name)).await;
        let _guard = lock.lock().await;
        let mirror = self
            .root
            .join("sources")
            .join(format!("{}.git", source_key(&definition.name)));
        self.sync_mirror(&mirror, &definition.repository).await?;
        let canonical_ref = format!("refs/heads/{}", definition.default_branch);
        let canonical_commit = git_output(
            self.git()
                .arg("--git-dir")
                .arg(&mirror)
                .arg("rev-parse")
                .arg(&canonical_ref),
            "resolve current canonical package commit",
        )
        .await?;
        let integration_root = self
            .root
            .join("agents")
            .join(&submission.checkout_id)
            .join("integrations")
            .join(&submission.submission_id);
        let source = integration_root.join("source");
        tokio::fs::create_dir_all(&integration_root).await?;
        if tokio::fs::try_exists(&source).await? {
            tokio::fs::remove_dir_all(&source).await?;
        }
        run(
            self.git()
                .arg("clone")
                .arg("--no-hardlinks")
                .arg(&mirror)
                .arg(&source),
            "create isolated integration checkout",
        )
        .await?;
        run(
            self.git().arg("-C").arg(&source).args([
                "config",
                "user.name",
                "VM Package Controller",
            ]),
            "configure integration Git identity",
        )
        .await?;
        run(
            self.git().arg("-C").arg(&source).args([
                "config",
                "user.email",
                "packages@vm.internal",
            ]),
            "configure integration Git identity",
        )
        .await?;
        let submitted_source = self.checkout_source(&checkout)?;
        let submitted_ref = format!(
            "refs/submissions/{}",
            submission
                .submitted_commit
                .chars()
                .take(16)
                .collect::<String>()
        );
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("fetch")
                .arg(&submitted_source)
                .arg(format!("{submitted_ref}:{submitted_ref}")),
            "load approved submission into integration checkout",
        )
        .await?;
        let branch = format!("integration/{}", submission.submission_id);
        match request.strategy.as_str() {
            "rebase" => {
                run(
                    self.git()
                        .arg("-C")
                        .arg(&source)
                        .arg("switch")
                        .arg("--create")
                        .arg(&branch)
                        .arg(&submitted_ref),
                    "create rebased integration branch",
                )
                .await?;
                run(
                    self.git()
                        .arg("-C")
                        .arg(&source)
                        .arg("rebase")
                        .arg(&canonical_commit),
                    "rebase submission onto current canonical source",
                )
                .await?;
            }
            "merge" => {
                run(
                    self.git()
                        .arg("-C")
                        .arg(&source)
                        .arg("switch")
                        .arg("--create")
                        .arg(&branch)
                        .arg(&canonical_commit),
                    "create merged integration branch",
                )
                .await?;
                run(
                    self.git()
                        .arg("-C")
                        .arg(&source)
                        .args(["merge", "--no-ff", "--no-edit"])
                        .arg(&submitted_ref),
                    "merge submission into current canonical source",
                )
                .await?;
            }
            _ => unreachable!("strategy was validated"),
        }
        let integration_commit = git_output(
            self.git()
                .arg("-C")
                .arg(&source)
                .args(["rev-parse", "HEAD"]),
            "resolve integration commit",
        )
        .await?;
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("diff")
                .arg("--check")
                .arg(format!("{canonical_commit}..{integration_commit}")),
            "validate integrated diff",
        )
        .await?;
        let bundle = integration_root.join("integration.bundle");
        if tokio::fs::try_exists(&bundle).await? {
            tokio::fs::remove_file(&bundle).await?;
        }
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .args(["bundle", "create"])
                .arg(&bundle)
                .arg("--all"),
            "bundle integrated package source",
        )
        .await?;
        store
            .record_integration(
                &submission.submission_id,
                IntegrationRecord {
                    canonical_commit,
                    integration_commit,
                    strategy: request.strategy,
                    worktree: source.to_string_lossy().into_owned(),
                    validation: None,
                    timestamp: chrono::Utc::now(),
                },
                &request.actor,
                request.idempotency_key,
            )
            .await
    }

    pub fn integration_bundle(&self, submission: &SubmissionRecord) -> WorkResult<PathBuf> {
        let integration = submission
            .integration
            .as_ref()
            .ok_or_else(|| WorkError::Conflict("integration is not prepared".into()))?;
        let source = PathBuf::from(&integration.worktree);
        let expected = self
            .root
            .join("agents")
            .join(&submission.checkout_id)
            .join("integrations")
            .join(&submission.submission_id)
            .join("source");
        if source != expected {
            return Err(WorkError::Internal(
                "integration source escaped its managed directory".into(),
            ));
        }
        let bundle = expected
            .parent()
            .expect("managed integration source has a parent")
            .join("integration.bundle");
        if !bundle.is_file() {
            return Err(WorkError::Conflict("integration bundle is missing".into()));
        }
        Ok(bundle)
    }

    async fn prepare_locked(
        &self,
        store: &Store,
        checkout: &CheckoutRecord,
        definition: &vm_packages::PackageDefinition,
    ) -> WorkResult<CheckoutRecord> {
        let mirrors = self.root.join("sources");
        let checkout_root = self.root.join("agents").join(&checkout.checkout_id);
        let source = checkout_root.join("source");
        tokio::fs::create_dir_all(&mirrors).await?;
        tokio::fs::create_dir_all(&checkout_root).await?;

        let mirror = mirrors.join(format!("{}.git", source_key(&definition.name)));
        self.sync_mirror(&mirror, &definition.repository).await?;
        let reference = format!("refs/heads/{}", definition.default_branch);
        let base_commit = git_output(
            self.git()
                .arg("--git-dir")
                .arg(&mirror)
                .arg("rev-parse")
                .arg(&reference),
            "resolve package base commit",
        )
        .await?;

        if tokio::fs::try_exists(&source).await? {
            tokio::fs::remove_dir_all(&source).await?;
        }
        run(
            self.git()
                .arg("clone")
                .arg("--no-hardlinks")
                .arg(&mirror)
                .arg(&source),
            "create isolated package checkout",
        )
        .await?;
        let branch = format!(
            "agents/{}/{}",
            source_key(&checkout.agent),
            checkout.checkout_id
        );
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("switch")
                .arg("--create")
                .arg(&branch)
                .arg(&base_commit),
            "create package task branch",
        )
        .await?;
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("remote")
                .arg("set-url")
                .arg("origin")
                .arg(&definition.repository),
            "set canonical package remote",
        )
        .await?;

        store
            .record_source(
                &checkout.checkout_id,
                definition.default_branch.clone(),
                base_commit,
                branch,
                source.to_string_lossy().into_owned(),
            )
            .await
    }

    async fn sync_mirror(&self, mirror: &Path, repository: &str) -> WorkResult<()> {
        if mirror.is_dir() {
            return run(
                self.git()
                    .arg("--git-dir")
                    .arg(mirror)
                    .arg("remote")
                    .arg("update")
                    .arg("--prune"),
                "update canonical package mirror",
            )
            .await
            .map(|_| ());
        }

        let temporary = mirror.with_extension(format!(
            "tmp-{}",
            vm_core::secrets::generate_random_password(12)
        ));
        let clone = run(
            self.git()
                .arg("clone")
                .arg("--mirror")
                .arg(repository)
                .arg(&temporary),
            "clone canonical package mirror",
        )
        .await;
        if let Err(error) = clone {
            let _ = tokio::fs::remove_dir_all(&temporary).await;
            return Err(error);
        }
        tokio::fs::rename(temporary, mirror).await?;
        Ok(())
    }

    fn git(&self) -> Command {
        let mut command = Command::new("git");
        command.env("GIT_TERMINAL_PROMPT", "0");
        if let Ok(askpass) = std::env::var("PKG_WORK_GIT_ASKPASS") {
            command.env("GIT_ASKPASS", askpass);
        }
        if let Ok(token_file) = std::env::var("PKG_WORK_GIT_TOKEN_FILE") {
            command.env("PKG_WORK_GIT_TOKEN_FILE", token_file);
        }
        command
    }

    async fn lock(&self, key: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn checkout_source(&self, checkout: &CheckoutRecord) -> WorkResult<PathBuf> {
        let source = checkout
            .worktree
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| WorkError::Conflict("checkout source is not ready".into()))?;
        let expected = self
            .root
            .join("agents")
            .join(&checkout.checkout_id)
            .join("source");
        if source != expected {
            return Err(WorkError::Internal(
                "checkout source escaped its managed directory".into(),
            ));
        }
        Ok(source)
    }
}

async fn run(command: &mut Command, operation: &str) -> WorkResult<Output> {
    let output = command
        .output()
        .await
        .map_err(|error| WorkError::Internal(format!("{operation}: {error}")))?;
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(WorkError::Conflict(format!("{operation}: {stderr}")))
    }
}

async fn git_output(command: &mut Command, operation: &str) -> WorkResult<String> {
    let output = run(command, operation).await?;
    let value = String::from_utf8(output.stdout)
        .map_err(|_| WorkError::Internal(format!("{operation}: invalid UTF-8")))?;
    Ok(value.trim().to_string())
}

fn source_key(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let suffix = digest_hex(value.as_bytes());
    let suffix = &suffix[..8];
    format!("{}-{suffix}", slug.trim_matches('-'))
}

fn digest_hex(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::process::{Command as StdCommand, Stdio};

    use super::*;
    use vm_packages::{CreateCheckout, PackageEcosystem, RegisterPackage};

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
    async fn source_checkout_stays_inside_managed_agent_storage() {
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
        let submission = source
            .import_submission(&store, &active, &submitted_bundle)
            .await
            .unwrap();
        assert_eq!(submission.state, WorkflowState::Submitted);
        assert_ne!(submission.base_commit, submission.submitted_commit);
    }
}
