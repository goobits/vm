use chrono::Utc;
use vm_packages::{InternalPackageCatalog, PackageDefinition, RegisterPackage};

use crate::io::atomic_write;
use crate::store::{pretty_json, Database, SourceDefinition, Store};
use crate::{WorkError, WorkResult};
use vm_packages::SourceKind;

const CATALOG_FILE: &str = "catalog/packages.json";

impl Store {
    pub(crate) async fn materialize_catalog(&self) -> WorkResult<()> {
        let database = self.database.lock().await;
        self.materialize_catalog_locked(&database).await
    }

    async fn materialize_catalog_locked(&self, database: &Database) -> WorkResult<()> {
        let catalog = InternalPackageCatalog::from_definitions(database.packages.values())?;
        atomic_write(self.root().join(CATALOG_FILE), pretty_json(&catalog)?).await
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
                && existing.repository == request.repository
                && existing.default_branch == request.default_branch
            {
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
        self.database
            .lock()
            .await
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
        self.database
            .lock()
            .await
            .packages
            .values()
            .cloned()
            .collect()
    }

    pub async fn internal_catalog(&self) -> WorkResult<InternalPackageCatalog> {
        InternalPackageCatalog::from_definitions(self.database.lock().await.packages.values())
            .map_err(Into::into)
    }
}

pub(crate) fn source_definition(
    database: &Database,
    name: &str,
) -> WorkResult<Option<SourceDefinition>> {
    match (database.packages.get(name), database.tools.get(name)) {
        (Some(package), None) => Ok(Some(SourceDefinition {
            kind: SourceKind::Package,
            name: package.name.clone(),
            repository: package.repository.clone(),
            default_branch: package.default_branch.clone(),
        })),
        (None, Some(tool)) if tool.kind == vm_packages::ToolKind::Collection => {
            Ok(Some(SourceDefinition {
                kind: SourceKind::ToolCollection,
                name: tool.name.clone(),
                repository: tool.repository.clone(),
                default_branch: tool.default_branch.clone(),
            }))
        }
        (None, Some(_)) => Err(WorkError::Invalid(format!(
            "tool {name} is not an editable collection"
        ))),
        (Some(_), Some(_)) => Err(WorkError::Conflict(format!(
            "source name {name} is ambiguous between a package and tool"
        ))),
        (None, None) => Ok(None),
    }
}
