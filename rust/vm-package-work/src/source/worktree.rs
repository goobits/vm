use std::path::PathBuf;

use vm_packages::{
    CheckoutLease, CheckoutRecord, SubmissionRecord, TransitionRequest, WorkflowState,
};

use super::{git_output, run, source_key, SourceManager};
use crate::store::SourceDefinition;
use crate::{Store, WorkError, WorkResult};

impl SourceManager {
    /// Preserve the validated integration bundle outside mutable checkout data.
    /// The content-addressed file is never overwritten or removed by checkout cleanup.
    pub(crate) async fn retain_release_source(
        &self,
        submission: &SubmissionRecord,
    ) -> WorkResult<String> {
        let lock = self
            .lock(&format!("release:{}", submission.submission_id))
            .await;
        let _guard = lock.lock().await;
        let source = self.integration_bundle(submission)?;
        let digest_source = source.clone();
        let digest = tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(digest_source)?;
            vm_packages::sha256_reader(std::io::BufReader::new(file))
        })
        .await
        .map_err(|error| WorkError::Internal(format!("hash retained source archive: {error}")))??
        .0;
        let directory = self
            .root
            .join("agents/releases")
            .join(super::managed_component(
                "submission ID",
                &submission.submission_id,
            )?);
        tokio::fs::create_dir_all(&directory).await?;
        let destination = directory.join(format!("{digest}.bundle"));
        if tokio::fs::try_exists(&destination).await? {
            return Ok(digest);
        }
        let temporary = directory.join(format!(
            ".{digest}.{}",
            vm_core::secrets::generate_random_password(12)
        ));
        tokio::fs::copy(&source, &temporary).await?;
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&temporary)
            .await?;
        file.sync_all().await?;
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o444)).await?;
        }
        if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            if !tokio::fs::try_exists(&destination).await? {
                return Err(error.into());
            }
        }
        Ok(digest)
    }

    pub async fn prepare(
        &self,
        store: &Store,
        checkout: &CheckoutLease,
    ) -> WorkResult<CheckoutRecord> {
        if checkout.checkout.state != WorkflowState::Created {
            return Ok(checkout.checkout.clone());
        }
        let definition = store.source(&checkout.checkout.package).await?;
        if definition.kind != checkout.checkout.source_kind {
            return Err(WorkError::Conflict(
                "checkout source kind no longer matches the catalog".into(),
            ));
        }
        let lock = self.lock(&format!("source:{}", definition.name)).await;
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
        let source = self.checkout_source(checkout)?;
        if !source.is_dir() {
            return Err(WorkError::Conflict("checkout source is not ready".into()));
        }
        let archive = self
            .agent_root(&checkout.checkout_id)?
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

    /// Restore the appliance-side import target when post-integration review
    /// returns a compacted checkout to its assigned agent for rework.
    pub async fn restore_checkout(
        &self,
        store: &Store,
        checkout: &CheckoutRecord,
    ) -> WorkResult<()> {
        let definition = store.source(&checkout.package).await?;
        if definition.kind != checkout.source_kind {
            return Err(WorkError::Conflict(
                "checkout source kind no longer matches the catalog".into(),
            ));
        }
        let source_lock = self.lock(&format!("source:{}", definition.name)).await;
        let _source_guard = source_lock.lock().await;
        let checkout_lock = self
            .lock(&format!("checkout:{}", checkout.checkout_id))
            .await;
        let _checkout_guard = checkout_lock.lock().await;
        let source = self.checkout_source(checkout)?;
        if source.is_dir() {
            return Ok(());
        }
        if tokio::fs::try_exists(&source).await? {
            return Err(WorkError::Conflict(
                "checkout restore target is not a directory".into(),
            ));
        }

        let base_commit = checkout
            .base_commit
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("checkout base commit is missing".into()))?;
        let branch = checkout
            .branch
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("checkout branch is missing".into()))?;
        let mirror = self
            .root
            .join("sources")
            .join(format!("{}.git", source_key(&definition.name)));
        self.sync_mirror(&mirror, &definition.repository).await?;
        run(
            self.git()
                .arg("--git-dir")
                .arg(&mirror)
                .arg("cat-file")
                .arg("-e")
                .arg(format!("{base_commit}^{{commit}}")),
            "verify rework base commit",
        )
        .await?;
        if let Some(parent) = source.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let checkout_root = source
            .parent()
            .expect("managed checkout source has a parent")
            .to_path_buf();
        let temporary = source.with_extension(format!(
            "restore-{}",
            vm_core::secrets::generate_random_password(12)
        ));
        let restore = async {
            run(
                self.git()
                    .arg("clone")
                    .arg("--no-hardlinks")
                    .arg(&mirror)
                    .arg(&temporary),
                "restore isolated package checkout",
            )
            .await?;
            run(
                self.git()
                    .arg("-C")
                    .arg(&temporary)
                    .arg("switch")
                    .arg("--create")
                    .arg(branch)
                    .arg(base_commit),
                "restore package task branch",
            )
            .await?;
            let submission = match store.checkout_submission(&checkout.checkout_id).await {
                Ok(submission) => Some(submission),
                Err(WorkError::NotFound(_)) => None,
                Err(error) => return Err(error),
            };
            if let Some(submission) = submission {
                let key = submission
                    .submitted_commit
                    .chars()
                    .take(16)
                    .collect::<String>();
                let bundle = checkout_root
                    .join("submissions")
                    .join(format!("{key}.bundle"));
                if !tokio::fs::try_exists(&bundle).await? {
                    return Err(WorkError::Conflict(
                        "durable checkout submission bundle is missing".into(),
                    ));
                }
                run(
                    self.git()
                        .arg("-C")
                        .arg(&temporary)
                        .arg("fetch")
                        .arg(&bundle)
                        .arg(&submission.submitted_commit),
                    "restore submitted checkout commit",
                )
                .await?;
                run(
                    self.git()
                        .arg("-C")
                        .arg(&temporary)
                        .args(["reset", "--hard"])
                        .arg(&submission.submitted_commit),
                    "reset restored checkout to submitted commit",
                )
                .await?;
            }
            run(
                self.git()
                    .arg("-C")
                    .arg(&temporary)
                    .arg("remote")
                    .arg("set-url")
                    .arg("origin")
                    .arg(&definition.repository),
                "restore canonical package remote",
            )
            .await?;
            Ok::<(), WorkError>(())
        }
        .await;
        if let Err(error) = restore {
            let _ = tokio::fs::remove_dir_all(&temporary).await;
            return Err(error);
        }
        if let Err(error) = tokio::fs::rename(&temporary, &source).await {
            let _ = tokio::fs::remove_dir_all(&temporary).await;
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn cleanup_checkout(&self, checkout: &CheckoutRecord) -> WorkResult<()> {
        let lock = self
            .lock(&format!("checkout:{}", checkout.checkout_id))
            .await;
        let _guard = lock.lock().await;
        let checkout_root = self.agent_root(&checkout.checkout_id)?;
        if checkout.worktree.is_some() {
            self.checkout_source(checkout)?;
        }
        if tokio::fs::try_exists(&checkout_root).await? {
            tokio::fs::remove_dir_all(&checkout_root).await?;
        }
        Ok(())
    }

    /// Remove mutable Git worktrees after integration while retaining the
    /// immutable integration bundle required by the release job.
    pub(crate) async fn compact_integrated_checkout(
        &self,
        submission: &SubmissionRecord,
    ) -> WorkResult<()> {
        let lock = self
            .lock(&format!("checkout:{}", submission.checkout_id))
            .await;
        let _guard = lock.lock().await;
        let bundle = self.integration_bundle(submission)?;
        let integration_source = bundle
            .parent()
            .expect("managed integration bundle has a parent")
            .join("source");
        let checkout_root = self.agent_root(&submission.checkout_id)?;

        for directory in [checkout_root.join("source"), integration_source] {
            if tokio::fs::try_exists(&directory).await? {
                tokio::fs::remove_dir_all(directory).await?;
            }
        }
        for disposable in [
            checkout_root.join("source.bundle"),
            checkout_root.join("uploads"),
        ] {
            if !tokio::fs::try_exists(&disposable).await? {
                continue;
            }
            if disposable.is_dir() {
                tokio::fs::remove_dir_all(disposable).await?;
            } else {
                tokio::fs::remove_file(disposable).await?;
            }
        }
        Ok(())
    }

    async fn prepare_locked(
        &self,
        store: &Store,
        checkout: &CheckoutRecord,
        definition: &SourceDefinition,
    ) -> WorkResult<CheckoutRecord> {
        let mirrors = self.root.join("sources");
        let checkout_root = self.agent_root(&checkout.checkout_id)?;
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
}
