use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use vm_packages::{
    sha256_hex, tool_artifact_path, PublishToolArtifact, ToolBuild, ToolBuildRecord, ToolKind,
    ToolSourceManifest,
};

use crate::runtime::operation_key;

use super::archive::{
    build_collection, collection_links, confined_build_archive, verify_binary_archive,
    verify_binary_command,
};
use super::build::run_isolated;
use super::publication::publication_request;
use super::{git_text, RELEASE_ACTOR, TOOL_TARGET};

pub(super) struct BuiltToolArtifact {
    pub(super) archive: PathBuf,
    pub(super) manifest: ToolReleaseManifest,
    pub(super) request: PublishToolArtifact,
    pub(super) registry: String,
}

pub(super) struct ToolArtifactContext<'a> {
    pub(super) source: &'a Path,
    pub(super) release_root: &'a Path,
    pub(super) name: &'a str,
    pub(super) source_commit: &'a str,
    pub(super) tag: &'a str,
    pub(super) submission_id: &'a str,
    pub(super) gateway: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ToolReleaseManifest {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) target: String,
    pub(super) links: BTreeMap<String, String>,
    pub(super) source_commit: String,
    pub(super) tag: String,
    pub(super) actor: String,
    pub(super) idempotency_key: String,
}

pub(super) fn build_collection_artifact(
    context: &ToolArtifactContext<'_>,
) -> Result<BuiltToolArtifact> {
    let built = build_collection(context.source, &context.release_root.join("tool.tar.gz"))?;
    if built.source_commit != context.source_commit {
        bail!("collection archive source commit changed while building");
    }
    finish_artifact(
        built.archive,
        ToolReleaseManifest {
            name: context.name.into(),
            version: built.version,
            target: TOOL_TARGET.into(),
            links: collection_links(),
            source_commit: context.source_commit.into(),
            tag: context.tag.into(),
            actor: RELEASE_ACTOR.into(),
            idempotency_key: operation_key("tool-workflow", context.submission_id),
        },
        context.gateway,
    )
}

#[cfg(test)]
pub(super) fn build_binary_artifacts(
    context: &ToolArtifactContext<'_>,
    manifest: &ToolSourceManifest,
) -> Result<Vec<BuiltToolArtifact>> {
    let version = manifest
        .version
        .as_deref()
        .context("binary tool manifest has no version")?;
    let artifact_root = context.release_root.join("artifacts");
    std::fs::create_dir(&artifact_root)?;
    manifest
        .builds
        .iter()
        .map(|build| build_binary_artifact(context, version, &artifact_root, build))
        .collect()
}

pub(super) fn build_binary_artifact(
    context: &ToolArtifactContext<'_>,
    version: &str,
    artifact_root: &Path,
    build: &ToolBuild,
) -> Result<BuiltToolArtifact> {
    run_isolated(
        &build.command,
        context.source,
        context.release_root,
        &format!("build binary tool target {}", build.target),
    )?;
    let archive = confined_build_archive(context.source, &build.archive)?;
    verify_binary_archive(&archive, build)?;
    verify_binary_command(&archive, context.release_root, build)?;
    let retained = artifact_root.join(format!("{}.tar.gz", build.target));
    std::fs::copy(&archive, &retained)?;
    finish_artifact(
        retained,
        ToolReleaseManifest {
            name: context.name.into(),
            version: version.into(),
            target: build.target.clone(),
            links: build.links.clone(),
            source_commit: context.source_commit.into(),
            tag: context.tag.into(),
            actor: RELEASE_ACTOR.into(),
            idempotency_key: operation_key(
                "tool-workflow",
                &format!("{}:{}", context.submission_id, build.target),
            ),
        },
        context.gateway,
    )
}

pub(super) fn staged_binary_artifacts(
    context: &ToolArtifactContext<'_>,
    manifest: &ToolSourceManifest,
    build: &ToolBuildRecord,
    staging_root: &Path,
) -> Result<Vec<BuiltToolArtifact>> {
    let version = manifest
        .version
        .as_deref()
        .context("binary tool manifest has no version")?;
    let manifest_digest = sha256_hex(std::fs::read(context.source.join("vm-tool.yaml"))?);
    if !build.succeeded()
        || build.submission_id != context.submission_id
        || build.source_commit != context.source_commit
        || build.version != version
        || build.manifest_digest != manifest_digest
    {
        bail!("staged binary build does not match the validated source");
    }
    if build.artifacts.len() != manifest.builds.len() {
        bail!("staged binary build target count changed");
    }
    let staging_root =
        std::fs::canonicalize(staging_root).context("resolve managed binary build staging root")?;
    manifest
        .builds
        .iter()
        .map(|expected| {
            let artifact = build
                .artifacts
                .iter()
                .find(|artifact| artifact.target == expected.target)
                .context("staged binary build target is missing")?;
            if artifact.links != expected.links {
                bail!("staged binary build activation links changed");
            }
            let archive = staging_root
                .join(context.submission_id)
                .join(format!("{}.tar.gz", artifact.artifact_digest));
            let metadata = std::fs::symlink_metadata(&archive)
                .context("staged binary build archive is missing")?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                bail!("staged binary build archive is not a regular file");
            }
            let canonical = std::fs::canonicalize(&archive)?;
            if !canonical.starts_with(&staging_root) {
                bail!("staged binary build archive escaped managed storage");
            }
            verify_binary_archive(&canonical, expected)?;
            let built = finish_artifact(
                canonical,
                ToolReleaseManifest {
                    name: context.name.into(),
                    version: version.into(),
                    target: expected.target.clone(),
                    links: expected.links.clone(),
                    source_commit: context.source_commit.into(),
                    tag: context.tag.into(),
                    actor: RELEASE_ACTOR.into(),
                    idempotency_key: operation_key(
                        "tool-workflow",
                        &format!("{}:{}", context.submission_id, expected.target),
                    ),
                },
                context.gateway,
            )?;
            if built.request.artifact_digest != artifact.artifact_digest
                || built.request.size_bytes != artifact.size_bytes
            {
                bail!("staged binary build bytes changed");
            }
            Ok(built)
        })
        .collect()
}

pub(super) fn binary_identity(source: &Path) -> Result<ToolSourceManifest> {
    let content = git_text(
        source,
        &["show", "HEAD:vm-tool.yaml"],
        "read binary tool release manifest",
    )?;
    let manifest: ToolSourceManifest =
        serde_yaml_ng::from_str(&content).context("vm-tool.yaml is invalid")?;
    manifest.validate()?;
    if manifest.kind != ToolKind::Binary {
        bail!("registered binary tool has a non-binary vm-tool.yaml");
    }
    Ok(manifest)
}

fn finish_artifact(
    archive: PathBuf,
    manifest: ToolReleaseManifest,
    gateway: &str,
) -> Result<BuiltToolArtifact> {
    let request = publication_request(&archive, manifest.clone())?;
    let registry = format!(
        "{}{}",
        gateway.trim_end_matches('/'),
        tool_artifact_path(
            &manifest.name,
            &request.version,
            &request.target,
            &request.artifact_digest,
        )
    );
    Ok(BuiltToolArtifact {
        archive,
        manifest,
        request,
        registry,
    })
}
