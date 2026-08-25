use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{validate_label, PackageDefinition, PackageEcosystem, PackageValidationError};

/// Unambiguous identity used by resolver policy and persisted catalog snapshots.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PackageIdentity {
    pub ecosystem: PackageEcosystem,
    pub name: String,
}

impl PackageIdentity {
    pub fn new(
        ecosystem: PackageEcosystem,
        name: impl Into<String>,
    ) -> Result<Self, PackageValidationError> {
        let name = normalize_name(ecosystem, &name.into());
        validate_label("package", &name)?;
        Ok(Self { ecosystem, name })
    }

    pub fn key(&self) -> String {
        format!("{}:{}", self.ecosystem, self.name)
    }

    pub fn matches_name(&self, name: &str) -> bool {
        Self::new(self.ecosystem, name).is_ok_and(|candidate| candidate.eq(self))
    }
}

/// Minimal, credential-free snapshot shared with package protocol services.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalPackageCatalog {
    #[serde(default)]
    packages: BTreeSet<PackageIdentity>,
}

impl InternalPackageCatalog {
    pub fn new(packages: impl IntoIterator<Item = PackageIdentity>) -> Self {
        Self {
            packages: packages.into_iter().collect(),
        }
    }

    pub fn from_definitions<'a>(
        definitions: impl IntoIterator<Item = &'a PackageDefinition>,
    ) -> Result<Self, PackageValidationError> {
        definitions
            .into_iter()
            .map(|definition| PackageIdentity::new(definition.ecosystem, &definition.name))
            .collect::<Result<Vec<_>, _>>()
            .map(Self::new)
    }

    pub fn contains(&self, package: &PackageIdentity) -> bool {
        self.packages.contains(package)
    }

    pub fn packages(&self) -> &BTreeSet<PackageIdentity> {
        &self.packages
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResolutionAvailability {
    pub development_override: OverrideAvailability,
    pub published_release: bool,
    pub cached_release: bool,
    pub internal_registry: bool,
    pub public_upstream: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OverrideAvailability {
    #[default]
    NotConfigured,
    Available,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionSource {
    DevelopmentOverride,
    PublishedRelease,
    Cache,
    InternalRegistry,
    PublicUpstream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    MissingOverride(PackageIdentity),
    InternalUnavailable(PackageIdentity),
    ExternalUnavailable(PackageIdentity),
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingOverride(package) => write!(
                formatter,
                "development override for '{}' is configured but unavailable",
                package.key()
            ),
            Self::InternalUnavailable(package) => write!(
                formatter,
                "internal package '{}' is unavailable; public fallback is blocked",
                package.key()
            ),
            Self::ExternalUnavailable(package) => {
                write!(
                    formatter,
                    "external package '{}' is unavailable",
                    package.key()
                )
            }
        }
    }
}

impl std::error::Error for ResolutionError {}

/// Pure source-selection policy shared by every ecosystem adapter.
#[derive(Debug, Clone, Default)]
pub struct PackageResolver {
    catalog: InternalPackageCatalog,
}

impl PackageResolver {
    pub fn new(catalog: InternalPackageCatalog) -> Self {
        Self { catalog }
    }

    pub fn resolve(
        &self,
        package: PackageIdentity,
        available: ResolutionAvailability,
    ) -> Result<ResolutionSource, ResolutionError> {
        match available.development_override {
            OverrideAvailability::Available => return Ok(ResolutionSource::DevelopmentOverride),
            OverrideAvailability::Missing => return Err(ResolutionError::MissingOverride(package)),
            OverrideAvailability::NotConfigured => {}
        }
        if available.published_release {
            return Ok(ResolutionSource::PublishedRelease);
        }
        if available.cached_release {
            return Ok(ResolutionSource::Cache);
        }
        if self.catalog.contains(&package) {
            return available
                .internal_registry
                .then_some(ResolutionSource::InternalRegistry)
                .ok_or(ResolutionError::InternalUnavailable(package));
        }
        available
            .public_upstream
            .then_some(ResolutionSource::PublicUpstream)
            .ok_or(ResolutionError::ExternalUnavailable(package))
    }
}

fn normalize_name(ecosystem: PackageEcosystem, name: &str) -> String {
    match ecosystem {
        PackageEcosystem::Npm => name.to_ascii_lowercase(),
        PackageEcosystem::Cargo => name.to_ascii_lowercase().replace('_', "-"),
        PackageEcosystem::Python => {
            let mut normalized = String::with_capacity(name.len());
            let mut separator = false;
            for character in name.chars().flat_map(char::to_lowercase) {
                if matches!(character, '-' | '_' | '.') {
                    if !separator {
                        normalized.push('-');
                    }
                    separator = true;
                } else {
                    normalized.push(character);
                    separator = false;
                }
            }
            normalized
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(ecosystem: PackageEcosystem, name: &str) -> PackageIdentity {
        PackageIdentity::new(ecosystem, name).unwrap()
    }

    #[test]
    fn identities_follow_native_registry_normalization() {
        assert_eq!(
            package(PackageEcosystem::Npm, "@Goobits/Auth").name,
            "@goobits/auth"
        );
        assert_eq!(
            package(PackageEcosystem::Cargo, "Goobits_Auth").name,
            "goobits-auth"
        );
        assert_eq!(
            package(PackageEcosystem::Python, "Goobits.Auth_core").name,
            "goobits-auth-core"
        );
        assert_eq!(
            package(PackageEcosystem::Python, "Django--REST..framework").name,
            "django-rest-framework"
        );
        assert!(package(PackageEcosystem::Cargo, "goobits-auth").matches_name("Goobits_Auth"));
        assert!(package(PackageEcosystem::Python, "goobits-auth").matches_name("Goobits.Auth"));
    }

    #[test]
    fn source_precedence_is_deterministic() {
        let internal = package(PackageEcosystem::Cargo, "goobits-auth");
        let resolver = PackageResolver::new(InternalPackageCatalog::new([internal.clone()]));

        let source = resolver
            .resolve(
                internal.clone(),
                ResolutionAvailability {
                    development_override: OverrideAvailability::Available,
                    published_release: true,
                    cached_release: true,
                    internal_registry: true,
                    public_upstream: true,
                },
            )
            .unwrap();
        assert_eq!(source, ResolutionSource::DevelopmentOverride);

        assert!(matches!(
            resolver.resolve(
                internal,
                ResolutionAvailability {
                    public_upstream: true,
                    ..Default::default()
                }
            ),
            Err(ResolutionError::InternalUnavailable(_))
        ));
    }

    #[test]
    fn only_external_packages_can_fall_back_publicly() {
        let resolver = PackageResolver::default();
        assert_eq!(
            resolver
                .resolve(
                    package(PackageEcosystem::Npm, "is-number"),
                    ResolutionAvailability {
                        public_upstream: true,
                        ..Default::default()
                    }
                )
                .unwrap(),
            ResolutionSource::PublicUpstream
        );
    }
}
