use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::warn;
use vm_packages::{
    InternalPackageCatalog, PackageEcosystem, PackageIdentity, PackageResolver,
    ResolutionAvailability, ResolutionSource,
};

use crate::{AppError, AppResult, InternalRegistryClient};

pub(crate) const CATALOG_REFRESH_INTERVAL: Duration = Duration::from_secs(15);
const CATALOG_FETCH_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug)]
enum CatalogSource {
    Standalone,
    File(PathBuf),
    Remote {
        client: Arc<InternalRegistryClient>,
        cache_path: PathBuf,
    },
}

#[derive(Debug)]
struct CachedCatalog {
    value: InternalPackageCatalog,
    loaded: bool,
}

/// Fast, last-known-good catalog loader around the shared resolver policy.
#[derive(Debug)]
pub struct ResolverService {
    source: CatalogSource,
    cache: Mutex<CachedCatalog>,
}

impl ResolverService {
    pub fn from_environment(
        data_dir: &Path,
        internal_client: Option<Arc<InternalRegistryClient>>,
    ) -> Self {
        if let Some(path) = std::env::var_os("PKG_SERVER_INTERNAL_CATALOG") {
            return Self::new(Some(PathBuf::from(path)));
        }
        match internal_client {
            Some(client) => Self::worker_edge(data_dir, client),
            None => Self::standalone(),
        }
    }

    pub fn worker_edge(data_dir: &Path, client: Arc<InternalRegistryClient>) -> Self {
        Self::with_source(CatalogSource::Remote {
            client,
            cache_path: data_dir.join("catalog/packages.json"),
        })
    }

    pub fn standalone() -> Self {
        Self::with_source(CatalogSource::Standalone)
    }

    pub fn new(catalog_path: Option<PathBuf>) -> Self {
        match catalog_path {
            Some(path) => Self::with_source(CatalogSource::File(path)),
            None => Self::standalone(),
        }
    }

    fn with_source(source: CatalogSource) -> Self {
        Self {
            source,
            cache: Mutex::new(CachedCatalog {
                value: InternalPackageCatalog::default(),
                loaded: false,
            }),
        }
    }

    /// Resolve a cache miss without allowing internal names to reach public registries.
    pub async fn resolve_missing(
        &self,
        ecosystem: PackageEcosystem,
        name: &str,
        internal_registry: bool,
    ) -> AppResult<ResolutionSource> {
        let package = PackageIdentity::new(ecosystem, name)
            .map_err(|error| AppError::BadRequest(error.to_string()))?;
        PackageResolver::new(self.catalog().await?)
            .resolve(
                package,
                ResolutionAvailability {
                    internal_registry,
                    public_upstream: true,
                    ..Default::default()
                },
            )
            .map_err(|error| AppError::Unavailable(error.to_string()))
    }

    pub async fn require_public_upstream(
        &self,
        ecosystem: PackageEcosystem,
        name: &str,
    ) -> AppResult<()> {
        match self.resolve_missing(ecosystem, name, false).await? {
            ResolutionSource::PublicUpstream => Ok(()),
            source => Err(AppError::InternalError(format!(
                "resolver selected unexpected source {source:?} for a public fallback"
            ))),
        }
    }

    async fn catalog(&self) -> AppResult<InternalPackageCatalog> {
        if matches!(self.source, CatalogSource::Standalone) {
            return Ok(InternalPackageCatalog::default());
        }
        let mut cache = self.cache.lock().await;
        if cache.loaded {
            return Ok(cache.value.clone());
        }
        let catalog = self
            .refresh_catalog()
            .await
            .map_err(AppError::Unavailable)?;
        cache.value = catalog;
        cache.loaded = true;
        Ok(cache.value.clone())
    }

    /// Refresh the last-known-good catalog without disrupting active readers.
    pub async fn refresh(&self) -> AppResult<()> {
        if matches!(self.source, CatalogSource::Standalone) {
            return Ok(());
        }
        let catalog = self
            .refresh_catalog()
            .await
            .map_err(AppError::Unavailable)?;
        let mut cache = self.cache.lock().await;
        cache.value = catalog;
        cache.loaded = true;
        Ok(())
    }

    async fn refresh_catalog(&self) -> Result<InternalPackageCatalog, String> {
        match &self.source {
            CatalogSource::Standalone => Ok(InternalPackageCatalog::default()),
            CatalogSource::File(path) => read_catalog(path).await,
            CatalogSource::Remote {
                client,
                cache_path,
            } => match tokio::time::timeout(CATALOG_FETCH_TIMEOUT, client.catalog()).await {
                Ok(Ok(catalog)) => {
                    if let Err(error) = persist_catalog(cache_path, &catalog).await {
                        warn!(
                            operation = "persist_catalog",
                            path = %cache_path.display(),
                            error = %error,
                            "package catalog snapshot persistence failed"
                        );
                    }
                    Ok(catalog)
                }
                Ok(Err(error)) => read_catalog(cache_path).await.map_err(|cache_error| {
                    format!(
                        "internal package catalog is unavailable ({error}); cached snapshot failed: {cache_error}"
                    )
                }),
                Err(_) => read_catalog(cache_path).await.map_err(|cache_error| {
                    format!(
                        "internal package catalog request timed out after 1 second; cached snapshot failed: {cache_error}"
                    )
                }),
            },
        }
    }
}

async fn read_catalog(path: &Path) -> Result<InternalPackageCatalog, String> {
    let content = tokio::fs::read(path)
        .await
        .map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&content).map_err(|error| format!("{}: {error}", path.display()))
}

async fn persist_catalog(path: &Path, catalog: &InternalPackageCatalog) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(catalog).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || vm_core::file_system::atomic_write(&path, &content))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Json, Router};
    use vm_packages::PackageIdentity;

    async fn catalog_server(
        catalog: InternalPackageCatalog,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/work/v1/catalog",
            get(move || {
                let catalog = catalog.clone();
                async move { Json(catalog) }
            }),
        );
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), task)
    }

    #[tokio::test]
    async fn registered_packages_fail_closed_and_external_packages_pass() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("packages.json");
        let catalog = InternalPackageCatalog::new([PackageIdentity::new(
            PackageEcosystem::Python,
            "goobits-auth",
        )
        .unwrap()]);
        tokio::fs::write(&path, serde_json::to_vec(&catalog).unwrap())
            .await
            .unwrap();
        let resolver = ResolverService::with_source(CatalogSource::File(path));

        assert!(matches!(
            resolver
                .require_public_upstream(PackageEcosystem::Python, "goobits_auth")
                .await,
            Err(AppError::Unavailable(_))
        ));
        resolver
            .require_public_upstream(PackageEcosystem::Python, "requests")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn configured_catalog_fails_closed_until_first_snapshot() {
        let resolver = ResolverService::with_source(CatalogSource::File(PathBuf::from(
            "/definitely/missing/packages.json",
        )));
        for _ in 0..2 {
            assert!(matches!(
                resolver
                    .require_public_upstream(PackageEcosystem::Npm, "anything")
                    .await,
                Err(AppError::Unavailable(_))
            ));
        }
    }

    #[tokio::test]
    async fn remote_catalog_survives_control_plane_outages() {
        let directory = tempfile::tempdir().unwrap();
        let package = PackageIdentity::new(PackageEcosystem::Npm, "@goobits/auth").unwrap();
        let (gateway, server) =
            catalog_server(InternalPackageCatalog::new([package.clone()])).await;
        let client = Arc::new(InternalRegistryClient::new(&gateway, "read").unwrap());
        let cache_path = directory.path().join("catalog/packages.json");
        let resolver = ResolverService::with_source(CatalogSource::Remote {
            client,
            cache_path: cache_path.clone(),
        });

        assert_eq!(
            resolver
                .resolve_missing(PackageEcosystem::Npm, &package.name, true)
                .await
                .unwrap(),
            ResolutionSource::InternalRegistry
        );
        assert!(cache_path.is_file());

        server.abort();
        let offline = ResolverService::with_source(CatalogSource::Remote {
            client: Arc::new(InternalRegistryClient::new(&gateway, "read").unwrap()),
            cache_path,
        });
        assert_eq!(
            offline
                .resolve_missing(PackageEcosystem::Npm, &package.name, true)
                .await
                .unwrap(),
            ResolutionSource::InternalRegistry
        );
        assert_eq!(
            offline
                .resolve_missing(PackageEcosystem::Npm, "react", true)
                .await
                .unwrap(),
            ResolutionSource::PublicUpstream
        );
    }
}
