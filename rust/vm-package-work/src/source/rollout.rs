use std::path::{Path, PathBuf};

use vm_packages::{RolloutRecord, RolloutState};

use super::{allowed_rollout_path, git_output, run, source_key, SourceManager};
use crate::{Store, WorkError, WorkResult};

impl SourceManager {
    pub async fn prepare_rollout(
        &self,
        store: &Store,
        rollout: &RolloutRecord,
    ) -> WorkResult<RolloutRecord> {
        if rollout.state != RolloutState::Created {
            return Ok(rollout.clone());
        }
        let consumer = store.consumer(&rollout.consumer).await?;
        let lock = self.lock(&format!("consumer:{}", consumer.name)).await;
        let _guard = lock.lock().await;
        let mirrors = self.root.join("sources/consumers");
        tokio::fs::create_dir_all(&mirrors).await?;
        let mirror = mirrors.join(format!("{}.git", source_key(&consumer.name)));
        self.sync_mirror(&mirror, &consumer.repository).await?;
        let canonical_ref = format!("refs/heads/{}", consumer.default_branch);
        let base_commit = git_output(
            self.git()
                .arg("--git-dir")
                .arg(&mirror)
                .arg("rev-parse")
                .arg(&canonical_ref),
            "resolve consumer rollout base commit",
        )
        .await?;
        let rollout_root = self.root.join("rollouts").join(&rollout.rollout_id);
        let source = rollout_root.join("source");
        tokio::fs::create_dir_all(&rollout_root).await?;
        if tokio::fs::try_exists(&source).await? {
            tokio::fs::remove_dir_all(&source).await?;
        }
        run(
            self.git()
                .arg("clone")
                .arg("--no-hardlinks")
                .arg(&mirror)
                .arg(&source),
            "create isolated consumer rollout checkout",
        )
        .await?;
        let branch = format!(
            "rollouts/{}/{}",
            source_key(&rollout.package),
            rollout.rollout_id
        );
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .args(["switch", "--create"])
                .arg(&branch)
                .arg(&base_commit),
            "create consumer rollout branch",
        )
        .await?;
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .args(["remote", "set-url", "origin"])
                .arg(&consumer.repository),
            "set canonical consumer remote",
        )
        .await?;
        store
            .record_rollout_source(
                &rollout.rollout_id,
                base_commit,
                branch,
                source.to_string_lossy().into_owned(),
            )
            .await
    }

    pub async fn rollout_bundle(&self, rollout: &RolloutRecord) -> WorkResult<PathBuf> {
        let lock = self.lock(&format!("rollout:{}", rollout.rollout_id)).await;
        let _guard = lock.lock().await;
        let source = self.rollout_source(rollout)?;
        if !source.is_dir() {
            return Err(WorkError::Conflict("rollout source is not ready".into()));
        }
        let bundle = source
            .parent()
            .expect("managed rollout source has a parent")
            .join("source.bundle");
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
            "bundle isolated consumer rollout",
        )
        .await?;
        Ok(bundle)
    }

    pub async fn rollout_staging_path(&self, rollout: &RolloutRecord) -> WorkResult<PathBuf> {
        let source = self.rollout_source(rollout)?;
        if !source.is_dir() {
            return Err(WorkError::Conflict("rollout source is not ready".into()));
        }
        let uploads = source
            .parent()
            .expect("managed rollout source has a parent")
            .join("uploads");
        tokio::fs::create_dir_all(&uploads).await?;
        Ok(uploads.join(format!(
            "{}.bundle",
            vm_core::secrets::generate_random_password(16)
        )))
    }

    pub async fn cleanup_rollout(&self, rollout: &RolloutRecord) -> WorkResult<()> {
        let lock = self.lock(&format!("rollout:{}", rollout.rollout_id)).await;
        let _guard = lock.lock().await;
        let source = self.rollout_source(rollout)?;
        let rollout_root = source
            .parent()
            .ok_or_else(|| WorkError::Internal("managed rollout has no parent".into()))?;
        if tokio::fs::try_exists(rollout_root).await? {
            tokio::fs::remove_dir_all(rollout_root).await?;
        }
        Ok(())
    }

    pub async fn import_rollout(
        &self,
        store: &Store,
        rollout: &RolloutRecord,
        bundle: &Path,
    ) -> WorkResult<RolloutRecord> {
        let lock = self.lock(&format!("rollout:{}", rollout.rollout_id)).await;
        let _guard = lock.lock().await;
        let source = self.rollout_source(rollout)?;
        let branch = rollout
            .branch
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("rollout branch is missing".into()))?;
        let base_commit = rollout
            .base_commit
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("rollout base commit is missing".into()))?;
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .args(["bundle", "verify"])
                .arg(bundle),
            "verify consumer rollout bundle",
        )
        .await?;
        let heads = git_output(
            self.git()
                .args(["bundle", "list-heads"])
                .arg(bundle)
                .arg(format!("refs/heads/{branch}")),
            "read consumer rollout bundle head",
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
            .ok_or_else(|| WorkError::Invalid("bundle does not contain the rollout branch".into()))?
            .to_string();
        if submitted_commit == base_commit {
            return Err(WorkError::Invalid(
                "rollout contains no dependency update commit".into(),
            ));
        }
        let submitted_ref = format!("refs/rollouts/{submitted_commit}");
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("fetch")
                .arg(bundle)
                .arg(format!("refs/heads/{branch}:{submitted_ref}")),
            "import consumer rollout commit",
        )
        .await?;
        run(
            self.git().arg("-C").arg(&source).args([
                "merge-base",
                "--is-ancestor",
                base_commit,
                &submitted_commit,
            ]),
            "verify consumer rollout ancestry",
        )
        .await?;
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("diff")
                .arg("--check")
                .arg(format!("{base_commit}..{submitted_commit}")),
            "validate consumer rollout diff",
        )
        .await?;
        let paths = git_output(
            self.git()
                .arg("-C")
                .arg(&source)
                .args(["diff", "--name-only"])
                .arg(format!("{base_commit}..{submitted_commit}")),
            "inspect consumer rollout paths",
        )
        .await?;
        let invalid = paths
            .lines()
            .find(|path| !allowed_rollout_path(rollout.ecosystem, path));
        if let Some(path) = invalid {
            return Err(WorkError::Invalid(format!(
                "rollout changed unrelated path '{path}'"
            )));
        }
        store
            .record_rollout_submission(&rollout.rollout_id, submitted_commit)
            .await
    }

    pub async fn push_rollout(&self, store: &Store, rollout: &RolloutRecord) -> WorkResult<()> {
        if rollout.state != RolloutState::Validating {
            return Err(WorkError::Conflict("rollout is not ready to push".into()));
        }
        let source = self.rollout_source(rollout)?;
        let branch = rollout
            .branch
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("rollout branch is missing".into()))?;
        let submitted_commit = rollout
            .submitted_commit
            .as_deref()
            .ok_or_else(|| WorkError::Conflict("rollout commit is missing".into()))?;
        let consumer = store.consumer(&rollout.consumer).await?;
        let reference = format!("refs/heads/{branch}");
        let remote = git_output(
            self.git()
                .args(["ls-remote", &consumer.repository, &reference]),
            "inspect consumer rollout branch",
        )
        .await?;
        if let Some(commit) = remote.split_whitespace().next() {
            if commit == submitted_commit {
                return Ok(());
            }
            return Err(WorkError::Conflict(
                "remote rollout branch already points to a different commit".into(),
            ));
        }
        run(
            self.git()
                .arg("-C")
                .arg(&source)
                .arg("push")
                .arg("origin")
                .arg(format!("{submitted_commit}:{reference}")),
            "push tested consumer rollout branch",
        )
        .await?;
        Ok(())
    }
}
