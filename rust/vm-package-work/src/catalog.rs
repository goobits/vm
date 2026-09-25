use chrono::Utc;
use vm_core::file_system::atomic_write_async;
use vm_packages::{
    repository_urls_equivalent, InternalPackageCatalog, PackageDefinition, RegisterPackage,
    SourceKind,
};

use crate::store::{pretty_json, Database, SourceDefinition, Store};
use crate::{WorkError, WorkResult};

const CATALOG_FILE: &str = "catalog/packages.json";

impl Store {
    pub(crate) async fn materialize_catalog(&self) -> WorkResult<()> {
        let database = self.database.lock().await;
        self.materialize_catalog_locked(&database).await
    }

    async fn materialize_catalog_locked(&self, database: &Database) -> WorkResult<()> {
        let catalog = InternalPackageCatalog::from_definitions(
            database
                .packages
                .values()
                .filter(|package| !database.removed_packages.contains(&package.name)),
        )?;
        atomic_write_async(self.root().join(CATALOG_FILE), pretty_json(&catalog)?).await?;
        Ok(())
    }

    pub async fn register_package(
        &self,
        request: RegisterPackage,
    ) -> WorkResult<PackageDefinition> {
        request.validate()?;
        let mut current = self.database.lock().await;
        if current.tools.contains_key(&request.name) {
            return Err(WorkError::Conflict(format!(
                "source '{}' is already registered as a tool",
                request.name
            )));
        }
        if let Some(existing) = current.packages.get(&request.name).cloned() {
            if existing.ecosystem == request.ecosystem
                && repository_urls_equivalent(&existing.repository, &request.repository)
                && existing.default_branch == request.default_branch
            {
                if current.removed_packages.contains(&request.name) {
                    let mut next = current.clone();
                    next.removed_packages.remove(&request.name);
                    let definition = next
                        .packages
                        .get_mut(&request.name)
                        .expect("package remains registered");
                    definition.workspace_release |= request.workspace_release;
                    let definition = definition.clone();
                    self.commit(&mut current, next).await?;
                    self.materialize_catalog_locked(&current).await?;
                    return Ok(definition);
                }
                if request.workspace_release && !existing.workspace_release {
                    let mut next = current.clone();
                    let definition = next
                        .packages
                        .get_mut(&request.name)
                        .expect("package remains registered");
                    definition.workspace_release = true;
                    let definition = definition.clone();
                    self.commit(&mut current, next).await?;
                    self.materialize_catalog_locked(&current).await?;
                    return Ok(definition);
                }
                self.materialize_catalog_locked(&current).await?;
                return Ok(existing);
            }
            return Err(WorkError::Conflict(format!(
                "package '{}' is already registered with different settings",
                request.name
            )));
        }
        let definition = PackageDefinition {
            name: request.name,
            ecosystem: request.ecosystem,
            repository: request.repository,
            default_branch: request.default_branch,
            workspace_release: request.workspace_release,
            registered_at: Utc::now(),
        };
        let mut next = current.clone();
        next.packages
            .insert(definition.name.clone(), definition.clone());
        self.commit(&mut current, next).await?;
        self.materialize_catalog_locked(&current).await?;
        Ok(definition)
    }

    pub async fn package(&self, name: &str) -> WorkResult<PackageDefinition> {
        let database = self.database.lock().await;
        if database.removed_packages.contains(name) {
            return Err(WorkError::NotFound(format!("package {name}")));
        }
        database
            .packages
            .get(name)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("package {name}")))
    }

    pub(crate) async fn source(&self, name: &str) -> WorkResult<SourceDefinition> {
        let database = self.database.lock().await;
        source_definition(&database, name)?
            .ok_or_else(|| WorkError::NotFound(format!("source {name}")))
    }

    pub async fn packages(&self) -> Vec<PackageDefinition> {
        let database = self.database.lock().await;
        database
            .packages
            .values()
            .filter(|package| !database.removed_packages.contains(&package.name))
            .cloned()
            .collect()
    }

    pub async fn internal_catalog(&self) -> WorkResult<InternalPackageCatalog> {
        let database = self.database.lock().await;
        InternalPackageCatalog::from_definitions(
            database
                .packages
                .values()
                .filter(|package| !database.removed_packages.contains(&package.name)),
        )
        .map_err(Into::into)
    }

    pub async fn remove_package(&self, name: &str) -> WorkResult<()> {
        let mut current = self.database.lock().await;
        if !current.packages.contains_key(name) {
            return Err(WorkError::NotFound(format!("package {name}")));
        }
        if current.removed_packages.contains(name) {
            return self.materialize_catalog_locked(&current).await;
        }
        if current.consumers.values().any(|consumer| {
            !current.removed_consumers.contains(&consumer.name)
                && consumer.dependencies.contains_key(name)
        }) {
            return Err(WorkError::Conflict(format!(
                "package '{name}' is still declared by a registered consumer"
            )));
        }
        if current
            .checkouts
            .values()
            .any(|checkout| checkout.package == name && !checkout.state.is_terminal())
        {
            return Err(WorkError::Conflict(format!(
                "package '{name}' has an unfinished checkout"
            )));
        }
        if current.rollouts.values().any(|rollout| {
            rollout.package == name
                && !matches!(
                    rollout.state,
                    vm_packages::RolloutState::Closed
                        | vm_packages::RolloutState::Cancelled
                        | vm_packages::RolloutState::Failed
                )
        }) {
            return Err(WorkError::Conflict(format!(
                "package '{name}' has unfinished consumer updates"
            )));
        }
        let mut next = current.clone();
        next.removed_packages.insert(name.to_string());
        self.commit(&mut current, next).await?;
        self.materialize_catalog_locked(&current).await
    }
}

pub(crate) fn source_definition(
    database: &Database,
    name: &str,
) -> WorkResult<Option<SourceDefinition>> {
    match (
        database
            .packages
            .get(name)
            .filter(|_| !database.removed_packages.contains(name)),
        database
            .tools
            .get(name)
            .filter(|_| !database.removed_tools.contains(name)),
    ) {
        (Some(package), None) => Ok(Some(SourceDefinition {
            kind: SourceKind::Package,
            name: package.name.clone(),
            repository: package.repository.clone(),
            default_branch: package.default_branch.clone(),
            workspace_release: package.workspace_release,
        })),
        (None, Some(tool)) => Ok(Some(SourceDefinition {
            kind: match tool.kind {
                vm_packages::ToolKind::Binary => SourceKind::ToolBinary,
                vm_packages::ToolKind::Collection => SourceKind::ToolCollection,
            },
            name: tool.name.clone(),
            repository: tool.repository.clone(),
            default_branch: tool.default_branch.clone(),
            workspace_release: tool.workspace_release,
        })),
        (Some(_), Some(_)) => Err(WorkError::Conflict(format!(
            "source name {name} is ambiguous between a package and tool"
        ))),
        (None, None) => Ok(None),
    }
}
