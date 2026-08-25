use serde::{Deserialize, Serialize};
use vm_core::error::{Result, VmError};

use crate::metadata::SnapshotMetadata;

const LEGACY_VERSION: &str = "1.0";
const CURRENT_VERSION: &str = "2.0";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ArchivePlatform {
    os: String,
    arch: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ArchiveManifest {
    version: String,
    snapshot_name: String,
    is_global: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    platform: Option<ArchivePlatform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    project_name: String,
    #[serde(default)]
    total_size_bytes: u64,
    #[serde(default)]
    services: usize,
    #[serde(default)]
    volumes: usize,
}

impl ArchiveManifest {
    pub(crate) fn new(
        runtime: &str,
        snapshot_name: &str,
        is_global: bool,
        metadata: &SnapshotMetadata,
    ) -> Self {
        Self {
            version: CURRENT_VERSION.to_string(),
            snapshot_name: snapshot_name.to_string(),
            is_global,
            runtime: Some(runtime.to_string()),
            platform: Some(ArchivePlatform {
                os: vm_platform::platform::operating_system().to_string(),
                arch: vm_platform::platform::architecture().to_string(),
            }),
            created_at: Some(metadata.created_at.to_rfc3339()),
            description: metadata.description.clone(),
            project_name: metadata.project_name.clone(),
            total_size_bytes: metadata.total_size_bytes,
            services: metadata.services.len(),
            volumes: metadata.volumes.len(),
        }
    }

    pub(crate) fn parse(content: &str) -> Result<Self> {
        let manifest: Self = serde_json::from_str(content)
            .map_err(|error| VmError::general(error, "Failed to parse manifest.json"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub(crate) fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|error| VmError::general(error, "Failed to serialize manifest"))
    }

    pub(crate) fn snapshot_name(&self) -> &str {
        &self.snapshot_name
    }

    pub(crate) fn is_global(&self) -> bool {
        self.is_global
    }

    pub(crate) fn project_name(&self) -> &str {
        if self.is_global {
            "global"
        } else {
            &self.project_name
        }
    }

    pub(crate) fn validate_current_platform(&self) -> Result<()> {
        self.validate_platform(
            vm_platform::platform::operating_system(),
            vm_platform::platform::architecture(),
        )
    }

    fn validate(&self) -> Result<()> {
        if !matches!(self.version.as_str(), LEGACY_VERSION | CURRENT_VERSION) {
            return Err(VmError::validation(
                format!("Unsupported snapshot archive version '{}'", self.version),
                Some(format!(
                    "Use an archive with manifest version {LEGACY_VERSION} or {CURRENT_VERSION}"
                )),
            ));
        }
        if self.snapshot_name.trim().is_empty() || self.project_name.trim().is_empty() {
            return Err(VmError::validation(
                "Snapshot archive manifest has an empty snapshot or project name",
                None::<String>,
            ));
        }
        if self.version == CURRENT_VERSION && self.platform.is_none() {
            return Err(VmError::validation(
                "Snapshot archive manifest v2.0 is missing platform metadata",
                None::<String>,
            ));
        }
        Ok(())
    }

    fn validate_platform(&self, current_os: &str, current_arch: &str) -> Result<()> {
        let Some(platform) = &self.platform else {
            tracing::warn!(
                "Legacy v1 snapshot archive has no platform metadata; proceeding without a compatibility guarantee."
            );
            return Ok(());
        };

        if platform.os != current_os || platform.arch != current_arch {
            return Err(VmError::validation(
                format!(
                    "Snapshot was exported for {}/{} but current platform is {}/{}",
                    platform.os, platform.arch, current_os, current_arch
                ),
                Some("Use a matching machine or re-export the snapshot on this platform"),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v1(platform: Option<ArchivePlatform>) -> ArchiveManifest {
        ArchiveManifest {
            version: LEGACY_VERSION.to_string(),
            snapshot_name: "stable".to_string(),
            is_global: false,
            runtime: None,
            platform,
            created_at: None,
            description: None,
            project_name: "demo".to_string(),
            total_size_bytes: 0,
            services: 0,
            volumes: 0,
        }
    }

    #[test]
    fn v2_manifest_round_trips_with_required_platform() {
        let mut manifest = v1(Some(ArchivePlatform {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
        }));
        manifest.version = CURRENT_VERSION.to_string();

        let parsed = ArchiveManifest::parse(&manifest.to_json_pretty().unwrap()).unwrap();
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn v2_manifest_requires_platform() {
        let mut manifest = v1(None);
        manifest.version = CURRENT_VERSION.to_string();

        assert!(ArchiveManifest::parse(&serde_json::to_string(&manifest).unwrap()).is_err());
    }

    #[test]
    fn legacy_v1_without_platform_remains_importable() {
        let manifest = ArchiveManifest::parse(&serde_json::to_string(&v1(None)).unwrap()).unwrap();
        assert!(manifest.validate_platform("linux", "x86_64").is_ok());
    }

    #[test]
    fn every_known_platform_mismatch_is_rejected() {
        let manifest = v1(Some(ArchivePlatform {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
        }));

        assert!(manifest.validate_platform("macos", "aarch64").is_err());
    }
}
