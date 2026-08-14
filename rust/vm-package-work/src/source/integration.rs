use std::path::PathBuf;

use vm_packages::{IntegrationRecord, IntegrationRequest, SubmissionRecord, WorkflowState};

use super::{git_output, managed_component, run, source_key, SourceManager};
use crate::{Store, WorkError, WorkResult};

impl SourceManager {
    pub async fn prepare_integration(
        &self,
        store: &Store,
        submission: &SubmissionRecord,
        request: IntegrationRequest,
    ) -> WorkResult<SubmissionRecord> {
        if submission.state == WorkflowState::Integrating {
            let integration = submission
                .integration
                .as_ref()
                .ok_or_else(|| WorkError::Conflict("integration record is missing".into()))?;
            if integration.strategy != request.strategy {
                return Err(WorkError::Conflict(
                    "integration retry changed the merge strategy".into(),
                ));
            }
            return Ok(submission.clone());
        }
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
        let definition = store.source(&submission.package).await?;
        let checkout = store.get_checkout(&submission.checkout_id).await?;
        if definition.kind != checkout.source_kind {
            return Err(WorkError::Conflict(
                "checkout source kind no longer matches the catalog".into(),
            ));
        }
        let lock = self.lock(&format!("source:{}", definition.name)).await;
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
            .agent_root(&submission.checkout_id)?
            .join("integrations")
            .join(managed_component(
                "submission ID",
                &submission.submission_id,
            )?);
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
            .agent_root(&submission.checkout_id)?
            .join("integrations")
            .join(managed_component(
                "submission ID",
                &submission.submission_id,
            )?)
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
}
