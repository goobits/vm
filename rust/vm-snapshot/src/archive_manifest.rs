use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vm_core::error::{Result, VmError};

use crate::metadata::SnapshotMetadata;

const CURRENT_VERSION: &str = "3.0";

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
    platform: ArchivePlatform,
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
    #[serde(default)]
    files_sha256: BTreeMap<String, String>,
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
            platform: ArchivePlatform {
                os: vm_platform::platform::operating_system().to_string(),
                arch: vm_platform::platform::architecture().to_string(),
            },
            created_at: Some(metadata.created_at.to_rfc3339()),
            description: metadata.description.clone(),
            project_name: metadata.project_name.clone(),
            total_size_bytes: metadata.total_size_bytes,
            services: metadata.services.len(),
            volumes: metadata.volumes.len(),
            files_sha256: BTreeMap::new(),
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

    pub(crate) fn runtime(&self) -> Result<&str> {
        match self.runtime.as_deref() {
            Some(runtime @ ("docker" | "podman" | "tart")) => Ok(runtime),
            _ => Err(VmError::validation(
                "Snapshot archive has an unsupported provider",
                None::<String>,
            )),
        }
    }

    pub(crate) fn record_files(&mut self, root: &std::path::Path) -> Result<()> {
        self.files_sha256 = crate::archive::file_checksums(root)?;
        Ok(())
    }

    pub(crate) fn validate_files(&self, root: &std::path::Path) -> Result<()> {
        if self.files_sha256.is_empty() {
            return Err(VmError::validation(
                "Snapshot archive has no file checksums",
                None::<String>,
            ));
        }
        if crate::archive::file_checksums(root)? != self.files_sha256 {
            return Err(VmError::validation(
                "Snapshot archive checksum mismatch",
                None::<String>,
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_runtime(&self, executable: &str) -> Result<()> {
        if self.runtime.as_deref() != Some(executable) {
            return Err(VmError::validation(
                format!("Snapshot requires provider '{}'", self.runtime.as_deref().unwrap_or("unknown")),
                Some(format!("Select the recorded provider before importing or restoring (selected: {executable})")),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_architecture(&self, architecture: &str) -> Result<()> {
        if self.platform.arch != architecture {
            return Err(VmError::validation(
                "Snapshot metadata architecture does not match the archive manifest",
                None::<String>,
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_current_platform(&self) -> Result<()> {
        self.validate_platform(
            vm_platform::platform::operating_system(),
            vm_platform::platform::architecture(),
        )
    }

    fn validate(&self) -> Result<()> {
        self.runtime()?;
        if self.version != CURRENT_VERSION {
            return Err(VmError::validation(
                format!("Unsupported snapshot archive version '{}'", self.version),
                Some(format!(
                    "Use an archive with manifest version {CURRENT_VERSION}"
                )),
            ));
        }
        if self.snapshot_name.trim().is_empty() || self.project_name.trim().is_empty() {
            return Err(VmError::validation(
                "Snapshot archive manifest has an empty snapshot or project name",
                None::<String>,
            ));
        }
        Ok(())
    }

    fn validate_platform(&self, current_os: &str, current_arch: &str) -> Result<()> {
        let platform = &self.platform;
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

    fn manifest() -> ArchiveManifest {
        ArchiveManifest {
            version: CURRENT_VERSION.to_string(),
            snapshot_name: "stable".to_string(),
            is_global: false,
            runtime: Some("docker".to_string()),
            platform: ArchivePlatform {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            },
            created_at: None,
            description: None,
            project_name: "demo".to_string(),
            total_size_bytes: 0,
            services: 0,
            volumes: 0,
            files_sha256: BTreeMap::new(),
        }
    }

    #[test]
    fn v3_manifest_round_trips_with_required_platform() {
        let manifest = manifest();

        let parsed = ArchiveManifest::parse(&manifest.to_json_pretty().unwrap()).unwrap();
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn v3_manifest_requires_platform() {
        let content = serde_json::to_string(&manifest())
            .unwrap()
            .replace(r#","platform":{"os":"linux","arch":"x86_64"}"#, "");
        assert!(ArchiveManifest::parse(&content).is_err());
    }

    #[test]
    fn older_manifest_is_rejected() {
        let content = serde_json::to_string(&manifest())
            .unwrap()
            .replace(r#""version":"3.0""#, r#""version":"2.0""#);
        assert!(ArchiveManifest::parse(&content).is_err());
    }

    #[test]
    fn every_known_platform_mismatch_is_rejected() {
        let manifest = manifest();

        assert!(manifest.validate_platform("macos", "aarch64").is_err());
    }

    #[test]
    fn rejects_modified_payload_and_wrong_provider() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("metadata.json"), b"original").unwrap();
        let mut manifest = manifest();
        manifest.runtime = Some("docker".to_string());
        manifest.record_files(directory.path()).unwrap();
        assert!(manifest.validate_files(directory.path()).is_ok());
        assert!(manifest.validate_runtime("podman").is_err());

        std::fs::write(directory.path().join("metadata.json"), b"tampered").unwrap();
        assert!(manifest.validate_files(directory.path()).is_err());
    }
}
