use chrono::Utc;
use vm_packages::{
    validate_label, validate_registry_url, BeginReleaseRequest, CompleteReleaseRequest,
    PublicationRecord, PublicationRequest, ReceiptKind, ReleaseRecord, ReleaseReworkRequest,
    ReviewDecision, SourceKind, WorkflowState,
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
        if !request.source_pushed && request.source_archive_digest.is_none() {
            return Err(WorkError::Conflict(
                "release source must be pushed externally or retained internally".into(),
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
            SourceKind::ToolBinary | SourceKind::ToolCollection => next
                .tools
                .get(&package)
                .map(|definition| definition.repository.clone()),
        }
        .ok_or_else(|| WorkError::Internal("release source definition is missing".into()))?;
        if checkout.source_kind == SourceKind::ToolBinary {
            let build = next
                .tool_builds
                .get(submission_id)
                .filter(|build| build.succeeded())
                .ok_or_else(|| {
                    WorkError::Conflict("binary release requires a successful durable build".into())
                })?;
            let build_digest = if build.artifacts.len() == 1 {
                build.artifacts[0].artifact_digest.clone()
            } else {
                vm_packages::sha256_hex(
                    build
                        .artifacts
                        .iter()
                        .map(|artifact| {
                            format!("{}\0{}\n", artifact.target, artifact.artifact_digest)
                        })
                        .collect::<String>(),
                )
            };
            if build.source_commit != request.source_commit
                || build.version != request.version
                || build_digest != request.artifact_digest
            {
                return Err(WorkError::Conflict(
                    "binary release request does not match its durable build".into(),
                ));
            }
        }
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
            if request.source_pushed {
                "source commit and release tag pushed; publication started"
            } else {
                "immutable source archive retained; publication started"
            },
            Some(if request.source_pushed {
                "source_pushed".into()
            } else {
                "source_retained".into()
            }),
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
            source_archive_digest: request.source_archive_digest,
            registry: request.registry,
            expected_publications: request.expected_publications,
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

    pub async fn latest_published_source_commit(&self, package: &str) -> Option<String> {
        self.database
            .lock()
            .await
            .releases
            .values()
            .filter(|release| {
                release.package == package && release.state == WorkflowState::Published
            })
            .max_by_key(|release| release.created_at)
            .map(|release| release.source_commit.clone())
    }

    pub async fn next_release(&self) -> Option<vm_packages::SubmissionRecord> {
        let database = self.database.lock().await;
        database
            .submissions
            .values()
            .filter(|submission| {
                matches!(
                    submission.state,
                    WorkflowState::ReadyToRelease | WorkflowState::Publishing
                )
            })
            .filter(|submission| {
                let Some(checkout) = database.checkouts.get(&submission.checkout_id) else {
                    return false;
                };
                if checkout.source_kind != SourceKind::ToolBinary {
                    return true;
                }
                database
                    .tool_builds
                    .get(&submission.submission_id)
                    .is_some_and(|build| {
                        build.succeeded()
                            && submission.integration.as_ref().is_some_and(|integration| {
                                build.source_commit == integration.integration_commit
                            })
                    })
            })
            .min_by_key(|submission| submission.updated_at)
            .cloned()
    }

    pub async fn request_release_rework(
        &self,
        submission_id: &str,
        request: ReleaseReworkRequest,
    ) -> WorkResult<vm_packages::SubmissionRecord> {
        validate_release_rework(&request)?;
        let fingerprint = operation_fingerprint("release_rework", Some(submission_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .submissions
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }

        let mut next = current.clone();
        let submission = next
            .submissions
            .get(submission_id)
            .ok_or_else(|| WorkError::NotFound(submission_id.to_string()))?;
        if submission.state != WorkflowState::ReadyToRelease || submission.release_id.is_some() {
            return Err(WorkError::Conflict(
                "only an unpublished ready release can request source changes".into(),
            ));
        }
        transition_records(
            &mut next,
            submission_id,
            WorkflowState::NeedsChanges,
            ReceiptKind::Release,
            &request.actor,
            &request.reason,
            Some("needs_changes".into()),
        )?;
        let review = next
            .submissions
            .get_mut(submission_id)
            .expect("submission remains present")
            .review
            .as_mut()
            .ok_or_else(|| WorkError::Conflict("release review is missing".into()))?;
        review.decision = ReviewDecision::NeedsChanges;
        review.reason = request.reason;
        review.required_followups = request.required_followups;
        review.reviewer = request.actor;
        review.timestamp = Utc::now();
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: submission_id.to_string(),
            },
        );
        let result = next
            .submissions
            .get(submission_id)
            .cloned()
            .expect("submission remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
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
        if release.expected_publications.is_empty() {
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
        } else if !release.expected_publications.iter().any(|target| {
            target.registry == request.registry && target.artifact_digest == request.artifact_digest
        }) {
            return Err(WorkError::Conflict(
                "publication was not declared for this release".into(),
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
        let publications_complete = if release.expected_publications.is_empty() {
            release
                .publications
                .iter()
                .any(|publication| publication.registry == release.registry)
        } else {
            release.expected_publications.iter().all(|expected| {
                release.publications.iter().any(|publication| {
                    publication.registry == expected.registry
                        && publication.artifact_digest == expected.artifact_digest
                })
            })
        };
        if !publications_complete {
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

fn validate_release_rework(request: &ReleaseReworkRequest) -> WorkResult<()> {
    validate_label("release rework actor", &request.actor)?;
    if request.reason.trim().is_empty() || request.reason.len() > 4_000 {
        return Err(WorkError::Invalid(
            "release rework reason must contain 1 to 4000 characters".into(),
        ));
    }
    if request
        .required_followups
        .iter()
        .any(|followup| followup.trim().is_empty() || followup.len() > 1_000)
    {
        return Err(WorkError::Invalid(
            "release rework followups must contain 1 to 1000 characters".into(),
        ));
    }
    validate_idempotency_key(&request.idempotency_key)
}

fn validate_begin(request: &BeginReleaseRequest) -> WorkResult<()> {
    validate_label("version", &request.version)?;
    validate_label("release tag", &request.tag)?;
    validate_label("release actor", &request.actor)?;
    validate_hex("source commit", &request.source_commit, &[40, 64])?;
    validate_hex("artifact digest", &request.artifact_digest, &[64])?;
    if let Some(digest) = &request.source_archive_digest {
        validate_hex("source archive digest", digest, &[64])?;
    }
    if !request.source_pushed && request.source_archive_digest.is_none() {
        return Err(WorkError::Invalid(
            "release requires an external source push or retained source archive".into(),
        ));
    }
    validate_registry_url(&request.registry)?;
    let mut registries = std::collections::BTreeSet::new();
    for target in &request.expected_publications {
        validate_registry_url(&target.registry)?;
        validate_hex(
            "expected publication digest",
            &target.artifact_digest,
            &[64],
        )?;
        if !registries.insert(&target.registry) {
            return Err(WorkError::Invalid(
                "expected publication registries must be unique".into(),
            ));
        }
    }
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
        PublicApiDiff, RegisterPackage, ReleaseReworkRequest, ReviewRequest, TransitionRequest,
        ValidationRequest, VersionRecommendation,
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
                workspace_release: false,
            })
            .await
            .unwrap();
        let checkout = store
            .create_checkout(CreateCheckout {
                package: "auth".into(),
                agent: "agent-1".into(),
                consumers: vec!["project-a".into()],
                task: "release auth".into(),
                workspace_release: false,
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
        let rework_request = ReleaseReworkRequest {
            reason: "release version is smaller than the reviewed change".into(),
            required_followups: vec!["bump the package version and resubmit".into()],
            actor: "release-service".into(),
            idempotency_key: "release-rework".into(),
        };
        let needs_changes = store
            .request_release_rework(&submission.submission_id, rework_request.clone())
            .await
            .unwrap();
        assert_eq!(needs_changes.state, WorkflowState::NeedsChanges);
        assert_eq!(
            needs_changes.review.unwrap().required_followups,
            ["bump the package version and resubmit"]
        );
        assert_eq!(
            store
                .request_release_rework(&submission.submission_id, rework_request)
                .await
                .unwrap()
                .state,
            WorkflowState::NeedsChanges
        );
        assert!(store.next_release().await.is_none());

        store
            .record_submission(
                &checkout.checkout_id,
                ImportedSubmission {
                    submitted_commit: "4444444444444444444444444444444444444444".into(),
                    diff_digest: "c".repeat(64),
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
                    idempotency_key: "validate-rework".into(),
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
                    reason: "compatible rework".into(),
                    required_followups: vec![],
                    merge_strategy: "rebase".into(),
                    reviewer: "reviewer".into(),
                    idempotency_key: "review-rework".into(),
                },
            )
            .await
            .unwrap();
        store
            .record_integration(
                &submission.submission_id,
                IntegrationRecord {
                    canonical_commit: "1111111111111111111111111111111111111111".into(),
                    integration_commit: "5555555555555555555555555555555555555555".into(),
                    strategy: "rebase".into(),
                    worktree: "/data/agents/reworked-integration".into(),
                    validation: None,
                    timestamp: Utc::now(),
                },
                "controller",
                "integrate-rework".into(),
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
                    idempotency_key: "complete-integration-rework".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            store.next_release().await.unwrap().submission_id,
            submission.submission_id
        );
        let registry = "http://gateway:8080/cargo/index/";
        let second_registry = "http://gateway:8080/tools/linux-amd64/";
        let release = store
            .begin_release(
                &submission.submission_id,
                BeginReleaseRequest {
                    version: "1.0.1".into(),
                    tag: "v1.0.1".into(),
                    source_commit: "5555555555555555555555555555555555555555".into(),
                    artifact_digest: "b".repeat(64),
                    source_pushed: false,
                    source_archive_digest: Some("d".repeat(64)),
                    registry: registry.into(),
                    expected_publications: vec![
                        vm_packages::PublicationTarget {
                            registry: registry.into(),
                            artifact_digest: "b".repeat(64),
                        },
                        vm_packages::PublicationTarget {
                            registry: second_registry.into(),
                            artifact_digest: "c".repeat(64),
                        },
                    ],
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
        assert!(store
            .complete_release(
                &release.release_id,
                CompleteReleaseRequest {
                    actor: "release-service".into(),
                    idempotency_key: "complete-too-early".into(),
                },
            )
            .await
            .is_err());
        let published = store
            .record_publication(
                &release.release_id,
                PublicationRequest {
                    registry: second_registry.into(),
                    artifact_digest: "c".repeat(64),
                    actor: "release-service".into(),
                    idempotency_key: "publish-second-release".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(published.publications.len(), 2);
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
