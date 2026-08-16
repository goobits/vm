use std::path::{Path, PathBuf};

use vm_packages::{sha256_hex, CheckoutRecord};

use super::{git_output, run, SourceManager};
use crate::{io::atomic_write, ImportedSubmission, Store, WorkError, WorkResult};

impl SourceManager {
    pub async fn submission_staging_path(&self, checkout: &CheckoutRecord) -> WorkResult<PathBuf> {
        let root = self.agent_root(&checkout.checkout_id)?;
        tokio::fs::create_dir_all(&root).await?;
        let uploads = root.join("uploads");
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
        if checkout.workspace_release {
            return self
                .import_workspace_submission(store, checkout, bundle)
                .await;
        }
        let lock = self
            .lock(&format!("checkout:{}", checkout.checkout_id))
            .await;
        let _guard = lock.lock().await;
        let source = self.checkout_source(checkout)?;
        let branch = checkout
            .branch
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("checkout branch is missing".into()))?;
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
        let submitted_commit = bundle_head(
            self,
            bundle,
            &format!("refs/heads/{branch}"),
            "bundle does not contain the checkout branch",
        )
        .await?;
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
        self.finalize_submission(store, checkout, bundle, &source, submitted_commit)
            .await
    }

    async fn import_workspace_submission(
        &self,
        store: &Store,
        checkout: &CheckoutRecord,
        bundle: &Path,
    ) -> WorkResult<vm_packages::SubmissionRecord> {
        if !matches!(
            checkout.state,
            vm_packages::WorkflowState::Created
                | vm_packages::WorkflowState::Active
                | vm_packages::WorkflowState::NeedsChanges
        ) {
            return Err(WorkError::Conflict(
                "workspace source can only bootstrap or replace an active submission".into(),
            ));
        }
        let lock = self
            .lock(&format!("checkout:{}", checkout.checkout_id))
            .await;
        let _guard = lock.lock().await;
        let submitted_commit = bundle_head(
            self,
            bundle,
            "HEAD",
            "workspace bundle does not contain HEAD",
        )
        .await?;
        let root = self.agent_root(&checkout.checkout_id)?;
        let source = root.join("source");
        let temporary = root.join(format!(
            "source-upload-{}",
            vm_core::secrets::generate_random_password(12)
        ));
        let prepare = async {
            run(
                self.git()
                    .arg("clone")
                    .arg("--no-hardlinks")
                    .arg(bundle)
                    .arg(&temporary),
                "clone submitted workspace source",
            )
            .await?;
            let cloned_commit = git_output(
                self.git()
                    .arg("-C")
                    .arg(&temporary)
                    .args(["rev-parse", "HEAD"]),
                "resolve submitted workspace commit",
            )
            .await?;
            if cloned_commit != submitted_commit {
                return Err(WorkError::Invalid(
                    "workspace bundle HEAD changed while importing".into(),
                ));
            }
            let (base_commit, initial_release) = match checkout.base_commit.as_deref() {
                Some(base_commit) => (base_commit.to_string(), checkout.initial_release),
                None => match store
                    .latest_published_source_commit(&checkout.package)
                    .await
                {
                    Some(base_commit) => (base_commit, false),
                    None => (submitted_commit.clone(), true),
                },
            };
            if !initial_release {
                run(
                    self.git()
                        .arg("-C")
                        .arg(&temporary)
                        .args(["merge-base", "--is-ancestor"])
                        .arg(&base_commit)
                        .arg(&submitted_commit),
                    "verify workspace submission ancestry",
                )
                .await
                .map_err(|_| {
                    WorkError::Conflict(
                        "workspace history does not contain the last internally published source commit"
                            .into(),
                    )
                })?;
            }
            let branch = checkout
                .branch
                .clone()
                .unwrap_or_else(|| format!("workspace/{}", checkout.checkout_id));
            run(
                self.git()
                    .arg("-C")
                    .arg(&temporary)
                    .args(["switch", "--force-create"])
                    .arg(&branch)
                    .arg(&submitted_commit),
                "create internal workspace submission branch",
            )
            .await?;
            Ok::<_, WorkError>((base_commit, branch, initial_release))
        }
        .await;
        let (base_commit, branch, initial_release) = match prepare {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = tokio::fs::remove_dir_all(&temporary).await;
                return Err(error);
            }
        };
        if tokio::fs::try_exists(&source).await? {
            tokio::fs::remove_dir_all(&source).await?;
        }
        tokio::fs::rename(&temporary, &source).await?;

        let active = if checkout.state == vm_packages::WorkflowState::Created {
            let definition = store.source(&checkout.package).await?;
            store
                .record_workspace_source(
                    &checkout.checkout_id,
                    definition.default_branch,
                    base_commit.clone(),
                    branch,
                    source.to_string_lossy().into_owned(),
                    initial_release,
                )
                .await?;
            store
                .transition(
                    &checkout.checkout_id,
                    vm_packages::TransitionRequest {
                        next: vm_packages::WorkflowState::Active,
                        actor: checkout.agent.clone(),
                        reason: "canonical workspace source retained internally".into(),
                        commit: Some(base_commit.clone()),
                        validation_result: Some("workspace_source_ready".into()),
                        idempotency_key: format!("workspace-active-{}", checkout.checkout_id),
                    },
                )
                .await?
        } else {
            checkout.clone()
        };
        let submission_ref = format!(
            "refs/submissions/{}",
            submitted_commit.chars().take(16).collect::<String>()
        );
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("update-ref")
                .arg(&submission_ref)
                .arg(&submitted_commit),
            "retain submitted workspace commit",
        )
        .await?;
        self.finalize_submission(store, &active, bundle, &source, submitted_commit)
            .await
    }

    async fn finalize_submission(
        &self,
        store: &Store,
        checkout: &CheckoutRecord,
        bundle: &Path,
        source: &Path,
        submitted_commit: String,
    ) -> WorkResult<vm_packages::SubmissionRecord> {
        let base_commit = checkout
            .base_commit
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("checkout base commit is missing".into()))?;
        if !checkout.initial_release && submitted_commit == base_commit {
            return Err(WorkError::Invalid(
                "submission contains no commits beyond its base".into(),
            ));
        }
        if !checkout.initial_release
            && run(
                self.git()
                    .arg("-C")
                    .arg(source)
                    .args(["merge-base", "--is-ancestor"])
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
        let diff_base = if checkout.initial_release {
            git_output(
                self.git().arg("-C").arg(source).args([
                    "hash-object",
                    "-t",
                    "tree",
                    "-w",
                    "--stdin",
                ]),
                "create empty initial-release tree",
            )
            .await?
        } else {
            base_commit.to_string()
        };
        let range = format!("{diff_base}..{submitted_commit}");
        run(
            self.git()
                .arg("-C")
                .arg(source)
                .args(["diff", "--check"])
                .arg(&range),
            "validate submitted diff",
        )
        .await?;
        let diff = run(
            self.git()
                .arg("-C")
                .arg(source)
                .args(["diff", "--binary"])
                .arg(range),
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
                    diff_digest: sha256_hex(&diff),
                },
            )
            .await
    }
}

async fn bundle_head(
    source: &SourceManager,
    bundle: &Path,
    reference: &str,
    missing: &str,
) -> WorkResult<String> {
    let heads = git_output(
        source
            .git()
            .arg("bundle")
            .arg("list-heads")
            .arg(bundle)
            .arg(reference),
        "read workspace bundle head",
    )
    .await?;
    heads
        .split_whitespace()
        .next()
        .filter(|commit| {
            matches!(commit.len(), 40 | 64)
                && commit
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
        })
        .map(str::to_string)
        .ok_or_else(|| WorkError::Invalid(missing.into()))
}
