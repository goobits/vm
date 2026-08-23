use std::collections::BTreeMap;
use std::path::{Component, Path};

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{validate_label, validate_repository_url, PackageValidationError};

/// The activation shape of a managed tool artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Binary,
    Collection,
}

/// Register one immutable tool release stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterTool {
    pub name: String,
    pub kind: ToolKind,
    pub repository: String,
    #[serde(default = "default_branch")]
    pub default_branch: String,
    /// Controller-attested membership in a configured package source root.
    #[serde(default)]
    pub workspace_release: bool,
}

impl RegisterTool {
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        validate_tool_name(&self.name)?;
        validate_label("default branch", &self.default_branch)?;
        validate_repository_url(&self.repository)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub kind: ToolKind,
    pub repository: String,
    pub default_branch: String,
    #[serde(default)]
    pub workspace_release: bool,
    pub registered_at: DateTime<Utc>,
}

pub const TOOL_SOURCE_SCHEMA: u32 = 1;

const fn default_tool_source_schema() -> u32 {
    TOOL_SOURCE_SCHEMA
}

/// Versioned source contract for managed tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSourceManifest {
    #[serde(default = "default_tool_source_schema")]
    pub schema: u32,
    pub kind: ToolKind,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub builds: Vec<ToolBuild>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolBuild {
    pub target: String,
    pub command: Vec<String>,
    pub archive: String,
    pub links: BTreeMap<String, String>,
    #[serde(default)]
    pub verify: Option<Vec<String>>,
}

impl ToolSourceManifest {
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        if self.schema != TOOL_SOURCE_SCHEMA {
            return Err(PackageValidationError::new(
                "unsupported tool manifest schema",
            ));
        }
        match self.kind {
            ToolKind::Collection => {
                if self.version.is_some() || !self.builds.is_empty() {
                    return Err(PackageValidationError::new(
                        "collection manifests cannot declare binary release fields",
                    ));
                }
            }
            ToolKind::Binary => {
                let version = self.version.as_deref().ok_or_else(|| {
                    PackageValidationError::new("binary tool manifest requires a version")
                })?;
                let version = validate_version(version)?;
                if !version.pre.is_empty() || !version.build.is_empty() {
                    return Err(PackageValidationError::new(
                        "binary tool version must be stable without build metadata",
                    ));
                }
                if self.builds.is_empty() || self.builds.len() > 16 {
                    return Err(PackageValidationError::new(
                        "binary tool manifest must define 1 to 16 builds",
                    ));
                }
                let mut targets = std::collections::BTreeSet::new();
                for build in &self.builds {
                    build.validate()?;
                    if !targets.insert(&build.target) {
                        return Err(PackageValidationError::new(
                            "binary tool build targets must be unique",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

impl ToolBuild {
    fn validate(&self) -> Result<(), PackageValidationError> {
        validate_tool_target(&self.target)?;
        if self.target == "any" {
            return Err(PackageValidationError::new(
                "binary tool builds require an OS and architecture target",
            ));
        }
        if !matches!(self.target.as_str(), "linux-amd64" | "linux-arm64") {
            return Err(PackageValidationError::new(
                "binary tool build target is not supported by the Linux release infrastructure",
            ));
        }
        validate_command("build command", &self.command)?;
        validate_relative_path("build archive", &self.archive)?;
        if !self.archive.ends_with(".tar.gz") {
            return Err(PackageValidationError::new(
                "binary tool build archive must end in .tar.gz",
            ));
        }
        validate_links(&self.links)?;
        if !self
            .links
            .keys()
            .any(|destination| destination.starts_with(".local/bin/"))
        {
            return Err(PackageValidationError::new(
                "binary tool build must define at least one .local/bin activation link",
            ));
        }
        if let Some(command) = &self.verify {
            validate_command("verification command", command)?;
            if !self.links.values().any(|source| source == &command[0]) {
                return Err(PackageValidationError::new(
                    "verification command must execute a linked artifact path",
                ));
            }
        }
        Ok(())
    }
}

fn validate_command(field: &str, command: &[String]) -> Result<(), PackageValidationError> {
    if command.is_empty() || command.len() > 64 {
        return Err(PackageValidationError::new(format!(
            "{field} must contain 1 to 64 arguments"
        )));
    }
    if command.iter().any(|argument| {
        argument.is_empty() || argument.len() > 4_096 || argument.contains(['\0', '\n', '\r'])
    }) {
        return Err(PackageValidationError::new(format!(
            "{field} contains an invalid argument"
        )));
    }
    let program = Path::new(&command[0])
        .file_name()
        .and_then(|program| program.to_str())
        .unwrap_or(&command[0]);
    let shell_command_text = command
        .iter()
        .skip(1)
        .take_while(|argument| *argument != "--")
        .any(|argument| {
            argument == "--command"
                || argument.strip_prefix('-').is_some_and(|flags| {
                    !flags.starts_with('-') && flags.chars().any(|flag| flag == 'c')
                })
        });
    if matches!(program, "sh" | "bash" | "dash" | "zsh" | "ksh" | "fish") && shell_command_text {
        return Err(PackageValidationError::new(format!(
            "{field} must not use shell command text"
        )));
    }
    Ok(())
}

/// Metadata recorded after the registry has accepted the immutable archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishToolArtifact {
    pub version: String,
    pub target: String,
    pub artifact_digest: String,
    pub size_bytes: u64,
    /// Guest-home-relative destination -> archive-relative source.
    pub links: BTreeMap<String, String>,
    pub source_commit: String,
    pub tag: String,
    pub actor: String,
    pub idempotency_key: String,
}

impl PublishToolArtifact {
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        validate_artifact_fields(
            &self.version,
            &self.target,
            &self.artifact_digest,
            self.size_bytes,
            &self.links,
        )?;
        validate_git_commit(&self.source_commit)?;
        validate_label("release tag", &self.tag)?;
        validate_label("actor", &self.actor)?;
        validate_label("idempotency key", &self.idempotency_key)
    }
}

/// An immutable tool artifact available through the package gateway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolArtifactRecord {
    pub tool: String,
    pub kind: ToolKind,
    pub version: String,
    pub target: String,
    pub artifact_digest: String,
    pub size_bytes: u64,
    pub links: BTreeMap<String, String>,
    pub source_repository: String,
    pub source_commit: String,
    pub tag: String,
    pub artifact_path: String,
    pub actor: String,
    pub published_at: DateTime<Utc>,
    pub receipt_id: String,
}

impl ToolArtifactRecord {
    pub fn artifact_key(&self) -> String {
        artifact_key(&self.tool, &self.version, &self.target)
    }

    /// Validate metadata loaded from a registry index or controller cache.
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        validate_tool_name(&self.tool)?;
        validate_artifact_fields(
            &self.version,
            &self.target,
            &self.artifact_digest,
            self.size_bytes,
            &self.links,
        )?;
        validate_repository_url(&self.source_repository)?;
        validate_git_commit(&self.source_commit)?;
        validate_label("release tag", &self.tag)?;
        validate_label("actor", &self.actor)?;
        validate_label("receipt ID", &self.receipt_id)?;
        let expected_path = tool_artifact_path(
            &self.tool,
            &self.version,
            &self.target,
            &self.artifact_digest,
        );
        if self.artifact_path != expected_path {
            return Err(PackageValidationError::new(
                "tool artifact path does not match its immutable identity",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPublicationReceipt {
    pub receipt_id: String,
    pub tool: String,
    pub kind: ToolKind,
    pub version: String,
    pub target: String,
    pub source_repository: String,
    pub source_commit: String,
    pub tag: String,
    pub artifact_digest: String,
    pub size_bytes: u64,
    pub actor: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInventory {
    pub definition: ToolDefinition,
    pub artifacts: Vec<ToolArtifactRecord>,
}

/// One target-specific, locally generated update snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolIndex {
    pub target: String,
    pub generated_at: DateTime<Utc>,
    pub tools: BTreeMap<String, ToolArtifactRecord>,
}

pub fn artifact_key(tool: &str, version: &str, target: &str) -> String {
    format!("{tool}@{version}#{target}")
}

pub fn tool_artifact_path(tool: &str, version: &str, target: &str, digest: &str) -> String {
    format!("/tools/artifacts/{tool}/{version}/{target}/{digest}.tar.gz")
}

pub fn validate_tool_name(value: &str) -> Result<(), PackageValidationError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(PackageValidationError::new("invalid tool name"))
    }
}

pub fn validate_tool_target(value: &str) -> Result<(), PackageValidationError> {
    let valid = value == "any"
        || (!value.is_empty()
            && value.len() <= 64
            && value.starts_with(|character: char| character.is_ascii_alphanumeric())
            && value.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            }));
    if valid {
        Ok(())
    } else {
        Err(PackageValidationError::new("invalid tool target"))
    }
}

pub fn validate_sha256(value: &str) -> Result<(), PackageValidationError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(PackageValidationError::new(
            "artifact digest must be a lowercase SHA-256 hex string",
        ))
    }
}

pub fn validate_version(value: &str) -> Result<Version, PackageValidationError> {
    Version::parse(value).map_err(|_| PackageValidationError::new("tool version must be semantic"))
}

fn validate_artifact_fields(
    version: &str,
    target: &str,
    digest: &str,
    size_bytes: u64,
    links: &BTreeMap<String, String>,
) -> Result<(), PackageValidationError> {
    validate_version(version)?;
    validate_tool_target(target)?;
    validate_sha256(digest)?;
    if size_bytes == 0 {
        return Err(PackageValidationError::new(
            "tool artifact size must be greater than zero",
        ));
    }
    validate_links(links)
}

fn validate_links(links: &BTreeMap<String, String>) -> Result<(), PackageValidationError> {
    if links.is_empty() {
        return Err(PackageValidationError::new(
            "tool artifact must define at least one activation link",
        ));
    }
    if links.len() > 64 {
        return Err(PackageValidationError::new(
            "tool artifact cannot define more than 64 activation links",
        ));
    }
    for (destination, source) in links {
        validate_relative_path("activation destination", destination)?;
        validate_activation_destination(destination)?;
        validate_relative_path("artifact source", source)?;
    }
    let destinations = links.keys().map(Path::new).collect::<Vec<_>>();
    for (index, destination) in destinations.iter().enumerate() {
        if destinations
            .iter()
            .skip(index + 1)
            .any(|other| destination.starts_with(other) || other.starts_with(destination))
        {
            return Err(PackageValidationError::new(
                "tool activation destinations cannot overlap",
            ));
        }
    }
    Ok(())
}

fn validate_git_commit(value: &str) -> Result<(), PackageValidationError> {
    if (7..=64).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(PackageValidationError::new("invalid source commit"))
    }
}

fn validate_relative_path(field: &str, value: &str) -> Result<(), PackageValidationError> {
    let path = Path::new(value);
    let valid = !value.is_empty()
        && value.len() <= 512
        && !value.contains('\0')
        && !path.is_absolute()
        && !value.ends_with('/')
        && !value.contains("//")
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if valid {
        Ok(())
    } else {
        Err(PackageValidationError::new(format!("invalid {field}")))
    }
}

fn validate_activation_destination(value: &str) -> Result<(), PackageValidationError> {
    let destination = Path::new(value);
    let allowed = [
        ".local/bin",
        ".local/share",
        ".agents/skills",
        ".claude/skills",
        ".codex/skills",
        ".gemini/skills",
        ".config/antigravity/skills",
    ]
    .iter()
    .map(Path::new)
    .any(|root| destination == root || destination.starts_with(root));
    if allowed {
        Ok(())
    } else {
        Err(PackageValidationError::new(
            "activation destination is outside managed guest tool locations",
        ))
    }
}

fn default_branch() -> String {
    "main".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publication() -> PublishToolArtifact {
        PublishToolArtifact {
            version: "1.2.3".into(),
            target: "linux-arm64".into(),
            artifact_digest: "a".repeat(64),
            size_bytes: 42,
            links: BTreeMap::from([(".local/bin/codex".into(), "bin/codex".into())]),
            source_commit: "0123456789abcdef".into(),
            tag: "v1.2.3".into(),
            actor: "release-service".into(),
            idempotency_key: "tool-codex-1.2.3-linux-arm64".into(),
        }
    }

    #[test]
    fn validates_binary_and_collection_layouts_with_one_shape() {
        publication().validate().unwrap();
        let mut collection = publication();
        collection.target = "any".into();
        collection.links = BTreeMap::from([(".codex/skills".into(), "skills".into())]);
        collection.validate().unwrap();
    }

    #[test]
    fn rejects_unsafe_identifiers_paths_and_digests() {
        assert!(validate_tool_name("../codex").is_err());
        assert!(validate_tool_target("linux/arm64").is_err());
        assert!(validate_sha256(&"A".repeat(64)).is_err());

        let mut request = publication();
        request.links = BTreeMap::from([("../../.ssh".into(), "bin/codex".into())]);
        assert!(request.validate().is_err());

        request.links = BTreeMap::from([(".ssh/authorized_keys".into(), "key".into())]);
        assert!(request.validate().is_err());
    }

    #[test]
    fn artifact_records_validate_their_immutable_path() {
        let request = publication();
        let mut record = ToolArtifactRecord {
            tool: "codex".into(),
            kind: ToolKind::Binary,
            version: request.version,
            target: request.target,
            artifact_digest: request.artifact_digest,
            size_bytes: request.size_bytes,
            links: request.links,
            source_repository: "https://example.com/codex.git".into(),
            source_commit: request.source_commit,
            tag: request.tag,
            artifact_path: String::new(),
            actor: request.actor,
            published_at: Utc::now(),
            receipt_id: "tool-receipt-00000001".into(),
        };
        record.artifact_path = tool_artifact_path(
            &record.tool,
            &record.version,
            &record.target,
            &record.artifact_digest,
        );
        record.validate().unwrap();

        record.artifact_path = "/tools/artifacts/other.tar.gz".into();
        assert!(record.validate().is_err());
    }

    #[test]
    fn binary_source_manifest_is_versioned_and_argument_safe() {
        let manifest: ToolSourceManifest = serde_yaml_ng::from_str(
            r#"
schema: 1
kind: binary
version: 1.2.3
builds:
  - target: linux-arm64
    command: [npm, run, build:linux-arm64]
    archive: dist/tool-linux-arm64.tar.gz
    links:
      .local/bin/tool: bin/tool
    verify: [bin/tool, --version]
"#,
        )
        .unwrap();
        manifest.validate().unwrap();

        let shell_command = serde_yaml_ng::from_str::<ToolSourceManifest>(
            "schema: 1\nkind: binary\nversion: 1.2.3\nbuilds:\n  - target: linux-arm64\n    command: npm run build\n    archive: dist/tool.tar.gz\n    links: {'.local/bin/tool': bin/tool}\n",
        );
        assert!(shell_command.is_err());

        let mut shell_array = manifest;
        for flags in ["-c", "-ec", "--command"] {
            shell_array.builds[0].command = vec!["sh".into(), flags.into(), "npm run build".into()];
            assert!(shell_array.validate().is_err());
        }

        shell_array.builds[0].command = vec!["npm".into(), "run".into(), "build".into()];
        shell_array.builds[0].target = "darwin-arm64".into();
        assert!(shell_array.validate().is_err());
    }

    #[test]
    fn binary_source_manifest_rejects_unsafe_or_ambiguous_artifacts() {
        let mut manifest = ToolSourceManifest {
            schema: TOOL_SOURCE_SCHEMA,
            kind: ToolKind::Binary,
            version: Some("1.0.0".into()),
            builds: vec![ToolBuild {
                target: "linux-amd64".into(),
                command: vec!["make".into()],
                archive: "../tool.tar.gz".into(),
                links: BTreeMap::from([(".local/bin/tool".into(), "bin/tool".into())]),
                verify: None,
            }],
        };
        assert!(manifest.validate().is_err());

        manifest.builds[0].archive = "dist/tool.tar.gz".into();
        manifest.builds.push(manifest.builds[0].clone());
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn collection_source_manifest_accepts_the_legacy_implicit_schema() {
        let legacy: ToolSourceManifest = serde_yaml_ng::from_str("kind: collection\n").unwrap();
        assert_eq!(legacy.schema, TOOL_SOURCE_SCHEMA);
        legacy.validate().unwrap();

        let manifest: ToolSourceManifest =
            serde_yaml_ng::from_str("schema: 1\nkind: collection\n").unwrap();
        manifest.validate().unwrap();
    }
}
