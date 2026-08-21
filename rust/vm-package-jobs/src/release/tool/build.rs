use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use vm_packages::{
    sha256_hex, validate_managed_id, CompleteToolBuildRequest, PackageInfrastructureClient,
    SubmissionRecord, ToolBuildArtifact, ToolKind, ToolSourceManifest, WorkflowState,
};

use crate::runtime::{download_bundle, operation_key, run_command};

use super::{
    binary_identity, build_binary_artifacts, prepare_isolated_package_configuration,
    publication_request, BuiltToolArtifact, ToolArtifactContext,
};
use crate::release::{git_text, source::clone_at};

/// Build an approved binary tool in the credential-separated builder and
/// persist immutable artifacts for the publisher.
pub async fn build_submission(
    client: &PackageInfrastructureClient,
    submission: &SubmissionRecord,
    build_token: &str,
    gateway: &str,
    staging_root: &Path,
) -> Result<()> {
    if submission.state != WorkflowState::ReadyToRelease {
        bail!("binary tool submission is not ready to build");
    }
    let checkout = client.checkout(&submission.checkout_id).await?;
    if checkout.source_kind != vm_packages::SourceKind::ToolBinary {
        bail!("build queue returned a non-binary tool submission");
    }
    let inventory = client.tool(&submission.package).await?;
    if inventory.definition.kind != ToolKind::Binary {
        bail!("binary checkout no longer matches its registered tool definition");
    }
    let integration = submission
        .integration
        .as_ref()
        .context("binary tool submission has no integration record")?;
    let release_root = tempfile::tempdir()?;
    let bundle = release_root.path().join("integration.bundle");
    download_bundle(
        &client.build_bundle_url(&submission.submission_id),
        build_token,
        &bundle,
    )?;
    let source = release_root.path().join("source");
    clone_at(&bundle, &source, &integration.integration_commit)?;
    let raw_manifest = match std::fs::read(source.join("vm-tool.yaml")) {
        Ok(manifest) => manifest,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    let manifest_digest = sha256_hex(&raw_manifest);

    let identity = (|| -> Result<(String, String, ToolSourceManifest)> {
        let manifest = binary_identity(&source)?;
        let version = manifest
            .version
            .clone()
            .context("binary tool manifest has no version")?;
        let source_commit = git_text(
            &source,
            &["rev-parse", "--verify", "HEAD^{commit}"],
            "resolve binary tool source commit",
        )?;
        if source_commit != integration.integration_commit {
            bail!("binary build source does not match the validated integration");
        }
        Ok((source_commit, version, manifest))
    })();

    let (source_commit, version, manifest) = match identity {
        Ok(identity) => identity,
        Err(error) => {
            record_build_failure(
                client,
                submission,
                &integration.integration_commit,
                &manifest_digest,
                &error,
            )
            .await?;
            return Ok(());
        }
    };
    prepare_isolated_package_configuration(release_root.path())?;
    prepare_unprivileged_build(release_root.path(), &source)?;
    let tag = format!("v{version}");
    let context = ToolArtifactContext {
        source: &source,
        release_root: release_root.path(),
        name: &submission.package,
        source_commit: &source_commit,
        tag: &tag,
        submission_id: &submission.submission_id,
        gateway,
    };
    let artifacts = match build_binary_artifacts(&context, &manifest) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            record_build_failure(client, submission, &source_commit, &manifest_digest, &error)
                .await?;
            return Ok(());
        }
    };

    let staged = stage_artifacts(staging_root, &submission.submission_id, &artifacts)?;
    let request = CompleteToolBuildRequest {
        source_commit,
        manifest_digest,
        version,
        artifacts: staged,
        failure: None,
        actor: "tool-build-service".into(),
        idempotency_key: operation_key(
            "tool-build",
            &format!(
                "{}:{}",
                submission.submission_id, integration.integration_commit
            ),
        ),
    };
    client
        .complete_tool_build(&submission.submission_id, &request)
        .await?;
    println!("{} build staged", submission.submission_id);
    Ok(())
}

async fn record_build_failure(
    client: &PackageInfrastructureClient,
    submission: &SubmissionRecord,
    source_commit: &str,
    manifest_digest: &str,
    error: &anyhow::Error,
) -> Result<()> {
    let reason = bounded_failure(&format!("binary build failed: {error:#}"));
    let request = CompleteToolBuildRequest {
        source_commit: source_commit.into(),
        manifest_digest: manifest_digest.into(),
        version: String::new(),
        artifacts: Vec::new(),
        failure: Some(reason),
        actor: "tool-build-service".into(),
        idempotency_key: operation_key(
            "tool-build-failed",
            &format!("{}:{source_commit}", submission.submission_id),
        ),
    };
    client
        .complete_tool_build(&submission.submission_id, &request)
        .await?;
    println!("{} requires build changes", submission.submission_id);
    Ok(())
}

fn bounded_failure(reason: &str) -> String {
    const LIMIT: usize = 4_000;
    if reason.len() <= LIMIT {
        return reason.to_string();
    }
    let mut end = LIMIT;
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason[..end].to_string()
}

fn stage_artifacts(
    root: &Path,
    submission_id: &str,
    artifacts: &[BuiltToolArtifact],
) -> Result<Vec<ToolBuildArtifact>> {
    validate_managed_id("submission ID", submission_id)?;
    let directory = root.join(submission_id);
    std::fs::create_dir_all(&directory)?;
    artifacts
        .iter()
        .map(|artifact| {
            let digest = &artifact.request.artifact_digest;
            let destination = directory.join(format!("{digest}.tar.gz"));
            if destination.exists() {
                let existing = publication_request(&destination, artifact.manifest.clone())?;
                if existing.artifact_digest != *digest {
                    bail!("staged tool artifact digest changed");
                }
            } else {
                let temporary = directory.join(format!(".{digest}.tmp"));
                std::fs::copy(&artifact.archive, &temporary)?;
                let copied = publication_request(&temporary, artifact.manifest.clone())?;
                if copied.artifact_digest != *digest {
                    let _ = std::fs::remove_file(&temporary);
                    bail!("copied tool artifact digest changed");
                }
                match std::fs::rename(&temporary, &destination) {
                    Ok(()) => {}
                    Err(error) if destination.exists() => {
                        let _ = std::fs::remove_file(&temporary);
                        let existing =
                            publication_request(&destination, artifact.manifest.clone())?;
                        if existing.artifact_digest != *digest {
                            return Err(error.into());
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(ToolBuildArtifact {
                target: artifact.request.target.clone(),
                artifact_digest: digest.clone(),
                size_bytes: artifact.request.size_bytes,
                links: artifact.request.links.clone(),
            })
        })
        .collect()
}

fn prepare_unprivileged_build(root: &Path, source: &Path) -> Result<()> {
    let Some(uid) = std::env::var_os("PKG_BUILD_UID") else {
        return Ok(());
    };
    let gid = std::env::var_os("PKG_BUILD_GID").context("PKG_BUILD_GID is required")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(root)?.permissions();
        permissions.set_mode(0o711);
        std::fs::set_permissions(root, permissions)?;
    }
    let sandbox = root.join("untrusted");
    for directory in [
        sandbox.join("cargo-home"),
        sandbox.join("cargo-target"),
        sandbox.join("npm-cache"),
        sandbox.join("pip-cache"),
        sandbox.join("xdg-cache"),
    ] {
        std::fs::create_dir_all(directory)?;
    }
    run_command(
        Command::new("chown")
            .arg("-R")
            .arg(format!(
                "{}:{}",
                uid.to_string_lossy(),
                gid.to_string_lossy()
            ))
            .arg(source)
            .arg(&sandbox),
        "prepare unprivileged binary build workspace",
    )?;
    Ok(())
}
