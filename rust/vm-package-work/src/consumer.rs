use chrono::Utc;
use vm_packages::{
    repository_urls_equivalent, validate_label, validate_repository_url, ConsumerRecord,
    ConsumerUsage, PackageDrift, RegisterConsumer, RolloutState, WorkflowState,
};

use crate::rollout::transition_rollout;
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
            if !repository_urls_equivalent(&existing.repository, &request.repository)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn consumer_registration_accepts_equivalent_github_transports() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        store
            .register_package(vm_packages::RegisterPackage {
                name: "auth".into(),
                ecosystem: vm_packages::PackageEcosystem::Npm,
                repository: "https://github.com/goobits/auth.git".into(),
                default_branch: "main".into(),
                workspace_release: false,
            })
            .await
            .unwrap();
        let request = RegisterConsumer {
            name: "project-a".into(),
            repository: "ssh://git@github.com/goobits/project-a.git".into(),
            default_branch: "main".into(),
            dependencies: std::collections::BTreeMap::from([("auth".into(), "1.0.0".into())]),
        };
        store.register_consumer(request.clone()).await.unwrap();

        let record = store
            .register_consumer(RegisterConsumer {
                repository: "https://github.com/goobits/project-a.git".into(),
                ..request
            })
            .await
            .unwrap();

        assert_eq!(
            record.repository,
            "ssh://git@github.com/goobits/project-a.git"
        );
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
