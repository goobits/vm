use chrono::Utc;
use vm_packages::{
    validate_label, validate_registry_url, BeginReleaseRequest, CompleteReleaseRequest,
    PublicationRecord, PublicationRequest, ReceiptKind, ReleaseRecord, ReviewDecision, SourceKind,
    WorkflowState,
};

use crate::store::{
    ensure_fingerprint, operation_fingerprint, receipt, validate_idempotency_key,
    IdempotencyRecord, ReceiptInput,
};
use crate::submission::transition_records;
use crate::{Store, WorkError, WorkResult};

impl Store {
    pub async fn begin_release(
        &self,
        submission_id: &str,
        request: BeginReleaseRequest,
    ) -> WorkResult<ReleaseRecord> {
        validate_begin(&request)?;
        let fingerprint = operation_fingerprint("begin_release", Some(submission_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .releases
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }

        let mut next = current.clone();
        let submission = next
            .submissions
            .get(submission_id)
            .ok_or_else(|| WorkError::NotFound(submission_id.to_string()))?;
        if submission.state != WorkflowState::ReadyToRelease {
            return Err(WorkError::Conflict(
                "only a validated integration can be released".into(),
            ));
        }
        if !submission
            .review
            .as_ref()
            .is_some_and(|review| review.decision == ReviewDecision::Approve)
        {
            return Err(WorkError::Conflict(
                "release requires an approved integration review".into(),
            ));
        }
        let integration_commit = &submission
            .integration
            .as_ref()
            .ok_or_else(|| WorkError::Conflict("integration record is missing".into()))?
            .integration_commit;
        if integration_commit != &request.source_commit {
            return Err(WorkError::Conflict(
                "release source commit does not match the validated integration".into(),
            ));
        }
        if !request.source_pushed {
            return Err(WorkError::Conflict(
                "source commit and tag must be pushed before publication".into(),
            ));
        }
        let checkout_id = submission.checkout_id.clone();
        let package = submission.package.clone();
        let checkout = next
            .checkouts
            .get(&checkout_id)
            .ok_or_else(|| WorkError::Internal("release checkout is missing".into()))?;
        let source_repository = match checkout.source_kind {
            SourceKind::Package => next
                .packages
                .get(&package)
                .map(|definition| definition.repository.clone()),
            SourceKind::ToolCollection => next
                .tools
                .get(&package)
                .filter(|definition| definition.kind == vm_packages::ToolKind::Collection)
                .map(|definition| definition.repository.clone()),
        }
        .ok_or_else(|| WorkError::Internal("release source definition is missing".into()))?;
        let release_id = format!("rel-{submission_id}");
        if next.releases.contains_key(&release_id) {
            return Err(WorkError::Conflict(
                "submission already has a release record".into(),
            ));
        }

        transition_records(
            &mut next,
            submission_id,
            WorkflowState::Publishing,
            ReceiptKind::Release,
            &request.actor,
            "source commit and release tag pushed; publication started",
            Some("source_pushed".into()),
        )?;
        let now = Utc::now();
        let release = ReleaseRecord {
            release_id: release_id.clone(),
            submission_id: submission_id.to_string(),
            checkout_id,
            package,
            version: request.version,
            source_repository,
            source_commit: request.source_commit,
            tag: request.tag,
            artifact_digest: request.artifact_digest,
            source_pushed: request.source_pushed,
            registry: request.registry,
            publications: Vec::new(),
            state: WorkflowState::Publishing,
            created_at: now,
            updated_at: now,
        };
        next.submissions
            .get_mut(submission_id)
            .expect("submission remains present")
            .release_id = Some(release_id.clone());
        next.releases.insert(release_id.clone(), release.clone());
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: release_id,
            },
        );
        self.commit(&mut current, next).await?;
        Ok(release)
    }

    pub async fn release(&self, release_id: &str) -> WorkResult<ReleaseRecord> {
        self.database
            .lock()
            .await
            .releases
            .get(release_id)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(release_id.to_string()))
    }

    pub async fn releases(&self) -> Vec<ReleaseRecord> {
        self.database
            .lock()
            .await
            .releases
            .values()
            .cloned()
            .collect()
    }

    pub async fn next_release(&self) -> Option<vm_packages::SubmissionRecord> {
        self.database
            .lock()
            .await
            .submissions
            .values()
            .filter(|submission| {
                matches!(
                    submission.state,
                    WorkflowState::ReadyToRelease | WorkflowState::Publishing
                )
            })
            .min_by_key(|submission| submission.updated_at)
            .cloned()
    }

    pub async fn record_publication(
        &self,
        release_id: &str,
        request: PublicationRequest,
    ) -> WorkResult<ReleaseRecord> {
        validate_publication(&request)?;
        let fingerprint = operation_fingerprint("publication", Some(release_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .releases
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let mut next = current.clone();
        let release = next
            .releases
            .get_mut(release_id)
            .ok_or_else(|| WorkError::NotFound(release_id.to_string()))?;
        if release.state != WorkflowState::Publishing {
            return Err(WorkError::Conflict("release is not publishing".into()));
        }
        if release.artifact_digest != request.artifact_digest {
            return Err(WorkError::Conflict(
                "published artifact digest does not match the release".into(),
            ));
        }
        if release.registry != request.registry {
            return Err(WorkError::Conflict(
                "publication registry was not declared for this release".into(),
            ));
        }
        if let Some(existing) = release
            .publications
            .iter()
            .find(|publication| publication.registry == request.registry)
        {
            if existing.artifact_digest == request.artifact_digest {
                return Ok(release.clone());
            }
            return Err(WorkError::Conflict(
                "registry already contains a different release artifact".into(),
            ));
        }
        let now = Utc::now();
        release.publications.push(PublicationRecord {
            registry: request.registry.clone(),
            artifact_digest: request.artifact_digest,
            published_at: now,
        });
        release
            .publications
            .sort_by(|left, right| left.registry.cmp(&right.registry));
        release.updated_at = now;
        let checkout_id = release.checkout_id.clone();
        let source_commit = release.source_commit.clone();
        let receipt = receipt(
            &mut next,
            ReceiptInput {
                kind: ReceiptKind::Publication,
                checkout_id: &checkout_id,
                actor: &request.actor,
                previous: Some(WorkflowState::Publishing),
                next: WorkflowState::Publishing,
                commit: Some(source_commit),
                validation_result: Some("artifact_digest_verified".into()),
                reason: &format!("immutable release published to {}", request.registry),
                timestamp: now,
            },
        );
        next.receipts.insert(receipt.receipt_id.clone(), receipt);
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: release_id.to_string(),
            },
        );
        let result = next
            .releases
            .get(release_id)
            .cloned()
            .expect("release remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn complete_release(
        &self,
        release_id: &str,
        request: CompleteReleaseRequest,
    ) -> WorkResult<ReleaseRecord> {
        validate_label("release actor", &request.actor)?;
        validate_idempotency_key(&request.idempotency_key)?;
        let fingerprint = operation_fingerprint("complete_release", Some(release_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .releases
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let mut next = current.clone();
        let release = next
            .releases
            .get(release_id)
            .ok_or_else(|| WorkError::NotFound(release_id.to_string()))?;
        if release.state != WorkflowState::Publishing {
            return Err(WorkError::Conflict("release is not publishing".into()));
        }
        if !release
            .publications
            .iter()
            .any(|publication| publication.registry == release.registry)
        {
            return Err(WorkError::Conflict(
                "not every declared registry publication has completed".into(),
            ));
        }
        let submission_id = release.submission_id.clone();
        transition_records(
            &mut next,
            &submission_id,
            WorkflowState::Published,
            ReceiptKind::Release,
            &request.actor,
            "release source, tag, and immutable publications completed",
            Some("published".into()),
        )?;
        let release = next
            .releases
            .get_mut(release_id)
            .expect("release remains present");
        release.state = WorkflowState::Published;
        release.updated_at = Utc::now();
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: release_id.to_string(),
            },
        );
        let result = next
            .releases
            .get(release_id)
            .cloned()
            .expect("release remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }
}

fn validate_begin(request: &BeginReleaseRequest) -> WorkResult<()> {
    validate_label("version", &request.version)?;
    validate_label("release tag", &request.tag)?;
    validate_label("release actor", &request.actor)?;
    validate_hex("source commit", &request.source_commit, &[40, 64])?;
    validate_hex("artifact digest", &request.artifact_digest, &[64])?;
    validate_registry_url(&request.registry)?;
    validate_idempotency_key(&request.idempotency_key)
}

fn validate_publication(request: &PublicationRequest) -> WorkResult<()> {
    validate_registry_url(&request.registry)?;
    validate_hex("artifact digest", &request.artifact_digest, &[64])?;
    validate_label("publication actor", &request.actor)?;
    validate_idempotency_key(&request.idempotency_key)
}

fn validate_hex(field: &str, value: &str, lengths: &[usize]) -> WorkResult<()> {
    if lengths.contains(&value.len())
        && value.chars().all(|character| character.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(WorkError::Invalid(format!("invalid {field}")))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::ImportedSubmission;
    use vm_packages::{
        CheckOutcome, CleanupRequest, CreateCheckout, IntegrationRecord, PackageEcosystem,
        PublicApiDiff, RegisterPackage, ReviewRequest, TransitionRequest, ValidationRequest,
        VersionRecommendation,
    };

    #[tokio::test]
    async fn release_publications_are_idempotent_durable_and_complete() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        store
            .register_package(RegisterPackage {
                name: "auth".into(),
                ecosystem: PackageEcosystem::Cargo,
                repository: "https://example.com/auth.git".into(),
                default_branch: "main".into(),
            })
            .await
            .unwrap();
        let checkout = store
            .create_checkout(CreateCheckout {
                package: "auth".into(),
                agent: "agent-1".into(),
                consumers: vec!["project-a".into()],
                task: "release auth".into(),
                lease_token: "lease-token-012345678901234567890123456789".into(),
                idempotency_key: "create-release".into(),
            })
            .await
            .unwrap()
            .checkout;
        store
            .record_source(
                &checkout.checkout_id,
                "main".into(),
                "1111111111111111111111111111111111111111".into(),
                "agents/one".into(),
                "/data/agents/one".into(),
            )
            .await
            .unwrap();
        store
            .transition(
                &checkout.checkout_id,
                TransitionRequest {
                    next: WorkflowState::Active,
                    actor: "agent-1".into(),
                    reason: "attached".into(),
                    commit: None,
                    validation_result: None,
                    idempotency_key: "activate-release".into(),
                },
            )
            .await
            .unwrap();
        let submission = store
            .record_submission(
                &checkout.checkout_id,
                ImportedSubmission {
                    submitted_commit: "2222222222222222222222222222222222222222".into(),
                    diff_digest: "a".repeat(64),
                },
            )
            .await
            .unwrap();
        store
            .validate_submission(
                &submission.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::from([("project-a".into(), CheckOutcome::Passed)]),
                    actor: "controller".into(),
                    idempotency_key: "validate-release".into(),
                },
            )
            .await
            .unwrap();
        store
            .record_review(
                &submission.submission_id,
                ReviewRequest {
                    decision: ReviewDecision::Approve,
                    recommended_version: VersionRecommendation::Patch,
                    api_diff: PublicApiDiff {
                        changed_paths: vec![],
                        potentially_breaking: false,
                    },
                    reason: "compatible".into(),
                    required_followups: vec![],
                    merge_strategy: "rebase".into(),
                    reviewer: "reviewer".into(),
                    idempotency_key: "review-release".into(),
                },
            )
            .await
            .unwrap();
        store
            .record_integration(
                &submission.submission_id,
                IntegrationRecord {
                    canonical_commit: "1111111111111111111111111111111111111111".into(),
                    integration_commit: "3333333333333333333333333333333333333333".into(),
                    strategy: "rebase".into(),
                    worktree: "/data/agents/integration".into(),
                    validation: None,
                    timestamp: Utc::now(),
                },
                "controller",
                "integrate-release".into(),
            )
            .await
            .unwrap();
        store
            .complete_integration(
                &submission.submission_id,
                ValidationRequest {
                    package: CheckOutcome::Passed,
                    consumers: BTreeMap::from([("project-a".into(), CheckOutcome::Passed)]),
                    actor: "controller".into(),
                    idempotency_key: "complete-integration-release".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            store.next_release().await.unwrap().submission_id,
            submission.submission_id
        );
        let registry = "http://gateway:8080/cargo/index/";
        let release = store
            .begin_release(
                &submission.submission_id,
                BeginReleaseRequest {
                    version: "1.0.1".into(),
                    tag: "v1.0.1".into(),
                    source_commit: "3333333333333333333333333333333333333333".into(),
                    artifact_digest: "b".repeat(64),
                    source_pushed: true,
                    registry: registry.into(),
                    actor: "release-service".into(),
                    idempotency_key: "begin-release".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            store.next_release().await.unwrap().submission_id,
            submission.submission_id
        );
        let published = store
            .record_publication(
                &release.release_id,
                PublicationRequest {
                    registry: registry.into(),
                    artifact_digest: "b".repeat(64),
                    actor: "release-service".into(),
                    idempotency_key: "publish-release".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(published.publications.len(), 1);
        let complete = store
            .complete_release(
                &release.release_id,
                CompleteReleaseRequest {
                    actor: "release-service".into(),
                    idempotency_key: "complete-release".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(complete.state, WorkflowState::Published);
        assert!(store.next_release().await.is_none());
        let closed = store
            .close_checkout(
                &checkout.checkout_id,
                CleanupRequest {
                    actor: "release-service".into(),
                    idempotency_key: "cleanup-release".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(closed.state, WorkflowState::Closed);
        assert_eq!(
            store
                .close_checkout(
                    &checkout.checkout_id,
                    CleanupRequest {
                        actor: "release-service".into(),
                        idempotency_key: "cleanup-release".into(),
                    },
                )
                .await
                .unwrap()
                .state,
            WorkflowState::Closed
        );
        drop(store);

        let reopened = Store::open(directory.path()).await.unwrap();
        assert_eq!(
            reopened.release(&release.release_id).await.unwrap().state,
            WorkflowState::Published
        );
        assert_eq!(
            reopened
                .get_checkout(&checkout.checkout_id)
                .await
                .unwrap()
                .state,
            WorkflowState::Closed
        );
        assert!(directory
            .path()
            .join("receipts/releases")
            .join(format!("{}.json", release.release_id))
            .is_file());
    }
}
