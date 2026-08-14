use std::path::{Path, PathBuf};

use vm_packages::{sha256_hex, CheckoutRecord};

use super::{git_output, run, SourceManager};
use crate::{io::atomic_write, ImportedSubmission, Store, WorkError, WorkResult};

impl SourceManager {
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
                    diff_digest: sha256_hex(&diff),
                },
            )
            .await
    }
}
