use std::path::PathBuf;

use vm_packages::{
    CheckoutLease, CheckoutRecord, SubmissionRecord, TransitionRequest, WorkflowState,
};

use super::{git_output, run, source_key, SourceManager};
use crate::store::SourceDefinition;
use crate::{Store, WorkError, WorkResult};

impl SourceManager {
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
