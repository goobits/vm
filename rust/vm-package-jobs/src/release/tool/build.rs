use std::path::Path;

use anyhow::{bail, Context, Result};
use semver::Version;
use vm_packages::{
    sha256_hex, CompleteToolBuildRequest, PackageInfrastructureClient, SubmissionRecord,
    ToolBuildFailureKind, ToolKind, ToolSourceManifest, VersionRecommendation, WorkflowState,
};

use crate::release::{git_text, source::clone_at};
use crate::runtime::{download_bundle, operation_key};

use super::super::workflow::validate_version_bump;
use super::artifact::{binary_identity, build_binary_artifacts, ToolArtifactContext};

mod sandbox;
mod sources;
mod staging;

use sandbox::prepare_unprivileged_build;
use sources::materialize;
use staging::stage;

#[cfg(test)]
pub(super) use sandbox::cargo_source_config;
pub(super) use sandbox::{native_target, prepare_isolated_package_configuration, run_isolated};

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
    let review = submission
        .review
        .as_ref()
        .context("binary tool submission has no integration review")?;
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
                ToolBuildFailureKind::Build,
            )
            .await?;
            return Ok(());
        }
    };
    let build_sources = match materialize(
        client,
        &submission.submission_id,
        build_token,
        release_root.path(),
        &manifest,
    ) {
        Ok(sources) => sources,
        Err(error) => {
            record_build_failure(
                client,
                submission,
                &source_commit,
                &manifest_digest,
                &error,
                ToolBuildFailureKind::Build,
            )
            .await?;
            return Ok(());
        }
    };
    if !checkout.initial_release {
        let canonical = release_root.path().join("canonical");
        clone_at(&bundle, &canonical, &integration.canonical_commit)?;
        let previous_version = binary_identity(&canonical)?
            .version
            .context("previous binary tool manifest has no version")?;
        if let Err(error) =
            validate_declared_version(&previous_version, &version, review.recommended_version)
        {
            record_build_failure(
                client,
                submission,
                &source_commit,
                &manifest_digest,
                &error,
                ToolBuildFailureKind::Version,
            )
            .await?;
            return Ok(());
        }
    }
    prepare_isolated_package_configuration(release_root.path())?;
    prepare_unprivileged_build(release_root.path(), &source, &build_sources)?;
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
            record_build_failure(
                client,
                submission,
                &source_commit,
                &manifest_digest,
                &error,
                ToolBuildFailureKind::Build,
            )
            .await?;
            return Ok(());
        }
    };

    let staged = stage(staging_root, &submission.submission_id, &artifacts)?;
    let request = CompleteToolBuildRequest {
        source_commit,
        manifest_digest,
        version,
        artifacts: staged,
        failure: None,
        failure_kind: None,
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
    kind: ToolBuildFailureKind,
) -> Result<()> {
    let reason = match kind {
        ToolBuildFailureKind::Build => format!("binary build failed: {error:#}"),
        ToolBuildFailureKind::Version => error.to_string(),
    };
    let request = CompleteToolBuildRequest {
        source_commit: source_commit.into(),
        manifest_digest: manifest_digest.into(),
        version: String::new(),
        artifacts: Vec::new(),
        failure: Some(bounded_failure(&reason)),
        failure_kind: Some(kind),
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

fn validate_declared_version(
    previous: &str,
    next: &str,
    recommendation: VersionRecommendation,
) -> Result<()> {
    let previous = Version::parse(previous).context("previous binary tool version is invalid")?;
    let next = Version::parse(next).context("binary tool version is invalid")?;
    validate_version_bump(&previous, &next, recommendation)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_version_fails_before_binary_build() {
        let error =
            validate_declared_version("1.1.0", "1.1.0", VersionRecommendation::Patch).unwrap_err();

        assert_eq!(
            error.to_string(),
            "release version 1.1.0 must be newer than 1.1.0"
        );
    }

    #[test]
    fn reviewed_patch_version_passes_preflight() {
        validate_declared_version("1.1.0", "1.1.1", VersionRecommendation::Patch).unwrap();
    }
}
