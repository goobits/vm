use chrono::Utc;
use vm_packages::{
    sha256_hex, validate_label, validate_repository_url, ConsumerRecord, ConsumerUsage,
    CreateRollout, PackageDrift, RegisterConsumer, RolloutRecord, RolloutState, RolloutTransition,
    RolloutValidationRequest, WorkflowState,
};

use crate::store::{
    ensure_fingerprint, next_id, operation_fingerprint, validate_idempotency_key, IdempotencyRecord,
};
use crate::{Store, WorkError, WorkResult};

impl Store {
    pub async fn register_consumer(&self, request: RegisterConsumer) -> WorkResult<ConsumerRecord> {
        validate_consumer(&request)?;
        let mut current = self.database.lock().await;
        for package in request.dependencies.keys() {
            if !current.packages.contains_key(package) {
                return Err(WorkError::Invalid(format!(
                    "consumer dependency '{package}' is not a registered shared package"
                )));
            }
        }
        if let Some(existing) = current.consumers.get(&request.name) {
            if existing.repository != request.repository
                || existing.default_branch != request.default_branch
            {
                return Err(WorkError::Conflict(format!(
                    "consumer '{}' is already registered with a different repository or branch",
                    request.name
                )));
            }
            if existing.dependencies == request.dependencies {
                return Ok(existing.clone());
            }

            let mut next = current.clone();
            let now = Utc::now();
            let consumer = next
                .consumers
                .get_mut(&request.name)
                .expect("consumer remains present");
            consumer.dependencies = request.dependencies;
            consumer.updated_at = now;
            let completed = next
                .rollouts
                .values()
                .filter(|rollout| {
                    rollout.consumer == request.name
                        && rollout.state == RolloutState::ReadyForReview
                        && consumer.dependencies.get(&rollout.package) == Some(&rollout.version)
                })
                .map(|rollout| rollout.rollout_id.clone())
                .collect::<Vec<_>>();
            for rollout_id in completed {
                transition_rollout(
                    next.rollouts
                        .get_mut(&rollout_id)
                        .expect("rollout remains present"),
                    RolloutState::Closed,
                    "consumer-inventory",
                    "reviewed consumer upgrade is now registered",
                    None,
                    Some("inventory_updated".into()),
                )?;
            }
            let result = next
                .consumers
                .get(&request.name)
                .cloned()
                .expect("consumer remains present");
            self.commit(&mut current, next).await?;
            return Ok(result);
        }
        let now = Utc::now();
        let record = ConsumerRecord {
            name: request.name,
            repository: request.repository,
            default_branch: request.default_branch,
            dependencies: request.dependencies,
            registered_at: now,
            updated_at: now,
        };
        let mut next = current.clone();
        next.consumers.insert(record.name.clone(), record.clone());
        self.commit(&mut current, next).await?;
        Ok(record)
    }

    pub async fn consumer(&self, name: &str) -> WorkResult<ConsumerRecord> {
        self.database
            .lock()
            .await
            .consumers
            .get(name)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("consumer {name}")))
    }

    pub async fn consumers(&self) -> Vec<ConsumerRecord> {
        self.database
            .lock()
            .await
            .consumers
            .values()
            .cloned()
            .collect()
    }

    pub async fn package_consumers(&self, package: &str) -> WorkResult<Vec<ConsumerUsage>> {
        let database = self.database.lock().await;
        if !database.packages.contains_key(package) {
            return Err(WorkError::NotFound(format!("package {package}")));
        }
        Ok(package_consumers(&database, package))
    }

    pub async fn drift(&self) -> Vec<PackageDrift> {
        let database = self.database.lock().await;
        database
            .packages
            .keys()
            .map(|package| PackageDrift {
                package: package.clone(),
                latest_version: latest_version(&database, package),
                consumers: package_consumers(&database, package),
            })
            .collect()
    }

    pub async fn create_rollout(&self, request: CreateRollout) -> WorkResult<RolloutRecord> {
        validate_rollout(&request)?;
        let fingerprint = operation_fingerprint("create_rollout", None, &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .rollouts
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        if let Some(existing) = current.rollouts.values().find(|rollout| {
            rollout.package == request.package
                && rollout.consumer == request.consumer
                && matches!(
                    rollout.state,
                    RolloutState::Created
                        | RolloutState::Active
                        | RolloutState::Validating
                        | RolloutState::ReadyForReview
                )
        }) {
            if existing.version == request.version {
                return Ok(existing.clone());
            }
            return Err(WorkError::Conflict(format!(
                "consumer '{}' already has a pending '{}' rollout",
                request.consumer, request.package
            )));
        }
        let package = current
            .packages
            .get(&request.package)
            .ok_or_else(|| WorkError::NotFound(format!("package {}", request.package)))?;
        let consumer = current
            .consumers
            .get(&request.consumer)
            .ok_or_else(|| WorkError::NotFound(format!("consumer {}", request.consumer)))?;
        let current_version = consumer.dependencies.get(&request.package).ok_or_else(|| {
            WorkError::Conflict(format!(
                "consumer '{}' does not declare package '{}'",
                request.consumer, request.package
            ))
        })?;
        if current_version == &request.version {
            return Err(WorkError::Conflict(
                "consumer already declares the requested package version".into(),
            ));
        }
        let released = current.releases.values().any(|release| {
            release.package == request.package
                && release.version == request.version
                && release.state == WorkflowState::Published
        });
        if !released {
            return Err(WorkError::Conflict(
                "rollout target is not a published immutable release".into(),
            ));
        }
        let mut next = current.clone();
        let now = Utc::now();
        let rollout_id = format!(
            "rollout-{}-{}-{:06}",
            branch_component(&request.package),
            now.format("%Y%m%d"),
            next_id(&mut next)
        );
        let receipt_id = format!("receipt-{rollout_id}-created");
        let record = RolloutRecord {
            rollout_id: rollout_id.clone(),
            package: request.package,
            version: request.version,
            consumer: request.consumer,
            ecosystem: package.ecosystem,
            state: RolloutState::Created,
            base_commit: None,
            branch: None,
            worktree: None,
            submitted_commit: None,
            created_at: now,
            updated_at: now,
            transitions: vec![RolloutTransition {
                timestamp: now,
                actor: request.actor,
                previous: None,
                next: RolloutState::Created,
                commit: None,
                validation_result: None,
                reason: "consumer rollout created".into(),
                receipt_id,
            }],
        };
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: rollout_id.clone(),
            },
        );
        next.rollouts.insert(rollout_id, record.clone());
        self.commit(&mut current, next).await?;
        Ok(record)
    }

    pub async fn rollout(&self, rollout_id: &str) -> WorkResult<RolloutRecord> {
        self.database
            .lock()
            .await
            .rollouts
            .get(rollout_id)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(rollout_id.to_string()))
    }

    pub async fn rollouts(&self) -> Vec<RolloutRecord> {
        self.database
            .lock()
            .await
            .rollouts
            .values()
            .cloned()
            .collect()
    }

    pub async fn ensure_automatic_rollouts(&self) -> WorkResult<Vec<RolloutRecord>> {
        let mut candidates = Vec::new();
        for package in self.drift().await {
            let Some(version) = package.latest_version else {
                continue;
            };
            for consumer in package.consumers {
                if consumer.version == version || consumer.pending_version.is_some() {
                    continue;
                }
                candidates.push(CreateRollout {
                    package: package.package.clone(),
                    version: version.clone(),
                    consumer: consumer.consumer.clone(),
                    actor: "package-rollout-service".into(),
                    idempotency_key: format!(
                        "auto-rollout-{}",
                        sha256_hex(format!(
                            "{}:{}:{}",
                            package.package, version, consumer.consumer
                        ))
                    ),
                });
            }
        }
        let mut rollouts = Vec::with_capacity(candidates.len());
        for request in candidates {
            rollouts.push(self.create_rollout(request).await?);
        }
        Ok(rollouts)
    }

    pub async fn next_rollout(&self) -> Option<RolloutRecord> {
        self.database
            .lock()
            .await
            .rollouts
            .values()
            .filter(|rollout| {
                matches!(
                    rollout.state,
                    RolloutState::Active | RolloutState::Validating
                )
            })
            .min_by_key(|rollout| rollout.updated_at)
            .cloned()
    }

    pub async fn record_rollout_source(
        &self,
        rollout_id: &str,
        base_commit: String,
        branch: String,
        worktree: String,
    ) -> WorkResult<RolloutRecord> {
        let mut current = self.database.lock().await;
        let mut next = current.clone();
        let rollout = next
            .rollouts
            .get_mut(rollout_id)
            .ok_or_else(|| WorkError::NotFound(rollout_id.to_string()))?;
        if rollout.state != RolloutState::Created {
            return Err(WorkError::Conflict(
                "rollout source can only be attached once".into(),
            ));
        }
        rollout.base_commit = Some(base_commit.clone());
        rollout.branch = Some(branch);
        rollout.worktree = Some(worktree);
        transition_rollout(
            rollout,
            RolloutState::Active,
            "package-controller",
            "isolated consumer upgrade source prepared",
            Some(base_commit),
            None,
        )?;
        let result = rollout.clone();
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn record_rollout_submission(
        &self,
        rollout_id: &str,
        submitted_commit: String,
    ) -> WorkResult<RolloutRecord> {
        let mut current = self.database.lock().await;
        let mut next = current.clone();
        let rollout = next
            .rollouts
            .get_mut(rollout_id)
            .ok_or_else(|| WorkError::NotFound(rollout_id.to_string()))?;
        if rollout.state != RolloutState::Active {
            return Err(WorkError::Conflict(
                "only an active rollout can be submitted".into(),
            ));
        }
        rollout.submitted_commit = Some(submitted_commit.clone());
        transition_rollout(
            rollout,
            RolloutState::Validating,
            "package-rollout-service",
            "consumer dependency and lockfile update submitted",
            Some(submitted_commit),
            Some("bundle_verified".into()),
        )?;
        let result = rollout.clone();
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn complete_rollout(
        &self,
        rollout_id: &str,
        request: RolloutValidationRequest,
    ) -> WorkResult<RolloutRecord> {
        validate_label("rollout actor", &request.actor)?;
        validate_idempotency_key(&request.idempotency_key)?;
        let fingerprint = operation_fingerprint("complete_rollout", Some(rollout_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .rollouts
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("idempotency target is missing".into()));
        }
        let mut next = current.clone();
        let rollout = next
            .rollouts
            .get_mut(rollout_id)
            .ok_or_else(|| WorkError::NotFound(rollout_id.to_string()))?;
        let target = if request.passed {
            RolloutState::ReadyForReview
        } else {
            RolloutState::Failed
        };
        transition_rollout(
            rollout,
            target,
            &request.actor,
            if request.passed {
                "consumer checks passed and upgrade branch was pushed"
            } else {
                "consumer checks failed"
            },
            rollout.submitted_commit.clone(),
            Some(if request.passed { "passed" } else { "failed" }.into()),
        )?;
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: rollout_id.to_string(),
            },
        );
        let result = next
            .rollouts
            .get(rollout_id)
            .cloned()
            .expect("rollout remains present");
        self.commit(&mut current, next).await?;
        Ok(result)
    }
}

fn package_consumers(database: &crate::store::Database, package: &str) -> Vec<ConsumerUsage> {
    database
        .consumers
        .values()
        .filter_map(|consumer| {
            consumer.dependencies.get(package).map(|version| {
                let pending = database
                    .rollouts
                    .values()
                    .filter(|rollout| {
                        rollout.package == package
                            && rollout.consumer == consumer.name
                            && matches!(
                                rollout.state,
                                RolloutState::Created
                                    | RolloutState::Active
                                    | RolloutState::Validating
                                    | RolloutState::ReadyForReview
                            )
                    })
                    .max_by_key(|rollout| rollout.created_at);
                ConsumerUsage {
                    consumer: consumer.name.clone(),
                    version: version.clone(),
                    pending_version: pending.map(|rollout| rollout.version.clone()),
                    rollout_id: pending.map(|rollout| rollout.rollout_id.clone()),
                }
            })
        })
        .collect()
}

fn latest_version(database: &crate::store::Database, package: &str) -> Option<String> {
    database
        .releases
        .values()
        .filter(|release| release.package == package && release.state == WorkflowState::Published)
        .max_by_key(|release| release.created_at)
        .map(|release| release.version.clone())
}

fn transition_rollout(
    rollout: &mut RolloutRecord,
    next: RolloutState,
    actor: &str,
    reason: &str,
    commit: Option<String>,
    validation_result: Option<String>,
) -> WorkResult<()> {
    let previous = rollout.state;
    let allowed = matches!(
        (previous, next),
        (
            RolloutState::Created,
            RolloutState::Active | RolloutState::Failed
        ) | (
            RolloutState::Active,
            RolloutState::Validating | RolloutState::Failed
        ) | (
            RolloutState::Validating,
            RolloutState::ReadyForReview | RolloutState::Failed
        ) | (
            RolloutState::ReadyForReview,
            RolloutState::Closed | RolloutState::Failed
        ) | (
            RolloutState::Failed | RolloutState::Cancelled,
            RolloutState::Closed
        )
    );
    if !allowed {
        return Err(WorkError::Conflict(format!(
            "cannot transition rollout from {previous:?} to {next:?}"
        )));
    }
    let now = Utc::now();
    let receipt_id = format!(
        "receipt-{}-{:03}",
        rollout.rollout_id,
        rollout.transitions.len() + 1
    );
    rollout.state = next;
    rollout.updated_at = now;
    rollout.transitions.push(RolloutTransition {
        timestamp: now,
        actor: actor.to_string(),
        previous: Some(previous),
        next,
        commit,
        validation_result,
        reason: reason.to_string(),
        receipt_id,
    });
    Ok(())
}

fn validate_consumer(request: &RegisterConsumer) -> WorkResult<()> {
    validate_label("consumer", &request.name)?;
    validate_label("consumer default branch", &request.default_branch)?;
    validate_repository_url(&request.repository)?;
    if request.dependencies.is_empty() {
        return Err(WorkError::Invalid(
            "consumer must declare at least one shared package dependency".into(),
        ));
    }
    for (package, version) in &request.dependencies {
        validate_label("package", package)?;
        validate_label("package version", version)?;
    }
    Ok(())
}

fn validate_rollout(request: &CreateRollout) -> WorkResult<()> {
    validate_label("package", &request.package)?;
    validate_label("package version", &request.version)?;
    validate_label("consumer", &request.consumer)?;
    validate_label("rollout actor", &request.actor)?;
    validate_idempotency_key(&request.idempotency_key)
}

fn branch_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_packages::{PackageEcosystem, PublicationRecord, RegisterPackage, ReleaseRecord};

    async fn store_with_auth_release(root: &std::path::Path, recorded_publication: bool) -> Store {
        let store = Store::open(root).await.unwrap();
        store
            .register_package(RegisterPackage {
                name: "auth".into(),
                ecosystem: PackageEcosystem::Cargo,
                repository: "https://example.com/auth.git".into(),
                default_branch: "main".into(),
            })
            .await
            .unwrap();
        let now = Utc::now();
        store.database.lock().await.releases.insert(
            "rel-auth".into(),
            ReleaseRecord {
                release_id: "rel-auth".into(),
                submission_id: "sub-auth".into(),
                checkout_id: "checkout-auth".into(),
                package: "auth".into(),
                version: "1.5.0".into(),
                source_repository: "https://example.com/auth.git".into(),
                source_commit: "a".repeat(40),
                tag: "v1.5.0".into(),
                artifact_digest: "b".repeat(64),
                source_pushed: true,
                registry: "https://packages.example/cargo/".into(),
                publications: recorded_publication
                    .then(|| PublicationRecord {
                        registry: "https://packages.example/cargo/".into(),
                        artifact_digest: "b".repeat(64),
                        published_at: now,
                    })
                    .into_iter()
                    .collect(),
                state: WorkflowState::Published,
                created_at: now,
                updated_at: now,
            },
        );
        store
    }

    fn auth_consumer(name: &str, version: &str) -> RegisterConsumer {
        RegisterConsumer {
            name: name.into(),
            repository: format!("https://example.com/{name}.git"),
            default_branch: "main".into(),
            dependencies: std::collections::BTreeMap::from([("auth".into(), version.into())]),
        }
    }

    #[tokio::test]
    async fn published_versions_automatically_queue_each_drifted_consumer_once() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_auth_release(directory.path(), false).await;
        for consumer in ["project-a", "project-b"] {
            store
                .register_consumer(auth_consumer(consumer, "1.4.0"))
                .await
                .unwrap();
        }

        let queued = store.ensure_automatic_rollouts().await.unwrap();
        assert_eq!(queued.len(), 2);
        assert!(queued.iter().all(|rollout| {
            rollout.version == "1.5.0" && rollout.state == RolloutState::Created
        }));
        assert!(store.ensure_automatic_rollouts().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn drift_keeps_consumers_pinned_while_rollout_is_pending() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_with_auth_release(directory.path(), true).await;
        for (consumer, version) in [("project-a", "1.4.2"), ("project-b", "1.3.8")] {
            store
                .register_consumer(auth_consumer(consumer, version))
                .await
                .unwrap();
        }
        let rollout = store
            .create_rollout(CreateRollout {
                package: "auth".into(),
                version: "1.5.0".into(),
                consumer: "project-a".into(),
                actor: "controller".into(),
                idempotency_key: "rollout-auth-a".into(),
            })
            .await
            .unwrap();
        store
            .record_rollout_source(
                &rollout.rollout_id,
                "c".repeat(40),
                "rollouts/auth/test".into(),
                format!("/data/rollouts/{}/source", rollout.rollout_id),
            )
            .await
            .unwrap();
        assert_eq!(
            store.next_rollout().await.unwrap().rollout_id,
            rollout.rollout_id
        );
        store
            .record_rollout_submission(&rollout.rollout_id, "d".repeat(40))
            .await
            .unwrap();
        store
            .complete_rollout(
                &rollout.rollout_id,
                RolloutValidationRequest {
                    passed: true,
                    actor: "rollout-service".into(),
                    idempotency_key: "complete-rollout-auth-a".into(),
                },
            )
            .await
            .unwrap();
        assert!(store.next_rollout().await.is_none());

        let consumers = store.package_consumers("auth").await.unwrap();
        assert_eq!(consumers[0].version, "1.4.2");
        assert_eq!(consumers[0].pending_version.as_deref(), Some("1.5.0"));
        assert_eq!(consumers[1].version, "1.3.8");
        assert!(consumers[1].pending_version.is_none());
        assert_eq!(
            store.drift().await[0].latest_version.as_deref(),
            Some("1.5.0")
        );
        assert!(directory
            .path()
            .join("receipts/rollouts")
            .join(format!("{}.json", rollout.rollout_id))
            .is_file());

        store
            .register_consumer(auth_consumer("project-a", "1.5.0"))
            .await
            .unwrap();
        let consumers = store.package_consumers("auth").await.unwrap();
        assert_eq!(consumers[0].version, "1.5.0");
        assert!(consumers[0].pending_version.is_none());
        assert_eq!(
            store.rollout(&rollout.rollout_id).await.unwrap().state,
            RolloutState::Closed
        );
    }
}
