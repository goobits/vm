use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tracing::warn;
use vm_packages::{
    InternalPackageCatalog, PackageEcosystem, PackageIdentity, PackageResolver,
    ResolutionAvailability, ResolutionSource,
};

use crate::{AppError, AppResult};

const CATALOG_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug)]
struct CachedCatalog {
    value: InternalPackageCatalog,
    loaded: bool,
    refreshed_at: Option<Instant>,
}

/// Fast, last-known-good catalog loader around the shared resolver policy.
#[derive(Debug)]
pub struct ResolverService {
    catalog_path: Option<PathBuf>,
    refresh_interval: Duration,
    cache: Mutex<CachedCatalog>,
}

impl ResolverService {
    pub fn from_environment() -> Self {
        Self::new(std::env::var_os("PKG_SERVER_INTERNAL_CATALOG").map(PathBuf::from))
    }

    pub fn standalone() -> Self {
        Self::new(None)
    }

    pub fn new(catalog_path: Option<PathBuf>) -> Self {
        Self::with_refresh_interval(catalog_path, CATALOG_REFRESH_INTERVAL)
    }

    fn with_refresh_interval(catalog_path: Option<PathBuf>, refresh_interval: Duration) -> Self {
        Self {
            catalog_path,
            refresh_interval,
            cache: Mutex::new(CachedCatalog {
                value: InternalPackageCatalog::default(),
                loaded: false,
                refreshed_at: None,
            }),
        }
    }

    /// Permit a public lookup only when the exact native package is not internal.
    pub async fn require_public_upstream(
        &self,
        ecosystem: PackageEcosystem,
        name: &str,
    ) -> AppResult<()> {
        let package = PackageIdentity::new(ecosystem, name)
            .map_err(|error| AppError::BadRequest(error.to_string()))?;
        let resolver = PackageResolver::new(self.catalog().await?);
        match resolver.resolve(
            package,
            ResolutionAvailability {
                public_upstream: true,
                ..Default::default()
            },
        ) {
            Ok(ResolutionSource::PublicUpstream) => Ok(()),
            Ok(source) => Err(AppError::InternalError(format!(
                "resolver selected unexpected source {source:?} for a public fallback"
            ))),
            Err(error) => Err(AppError::Unavailable(error.to_string())),
        }
    }

    async fn catalog(&self) -> AppResult<InternalPackageCatalog> {
        let Some(path) = self.catalog_path.as_ref() else {
            return Ok(InternalPackageCatalog::default());
        };
        let mut cache = self.cache.lock().await;
        if cache.loaded
            && cache
                .refreshed_at
                .is_some_and(|refreshed| refreshed.elapsed() < self.refresh_interval)
        {
            return Ok(cache.value.clone());
        }
        cache.refreshed_at = Some(Instant::now());

        match tokio::fs::read(path)
            .await
            .and_then(|content| serde_json::from_slice(&content).map_err(std::io::Error::other))
        {
            Ok(catalog) => {
                cache.value = catalog;
                cache.loaded = true;
                Ok(cache.value.clone())
            }
            Err(error) if cache.loaded => {
                warn!(path = %path.display(), error = %error, "using last-known package catalog");
                Ok(cache.value.clone())
            }
            Err(error) => Err(AppError::Unavailable(format!(
                "internal package catalog is unavailable at '{}': {error}",
                path.display()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_packages::PackageIdentity;

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
        let resolver = ResolverService::with_refresh_interval(Some(path), Duration::ZERO);

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
        let resolver = ResolverService::with_refresh_interval(
            Some(PathBuf::from("/definitely/missing/packages.json")),
            Duration::from_secs(60),
        );
        for _ in 0..2 {
            assert!(matches!(
                resolver
                    .require_public_upstream(PackageEcosystem::Npm, "anything")
                    .await,
                Err(AppError::Unavailable(_))
            ));
        }
    }
}
