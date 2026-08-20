use anyhow::{bail, Context, Result};
use vm_packages::{
    BeginReleaseRequest, PackageInfrastructureClient, PublicationRequest, RegistryEndpoints,
    SourceKind, WorkflowState,
};

use crate::runtime::{download_bundle, operation_key, required_secret as secret};

mod artifact;
mod manifest;

use artifact::{
    build_artifact, ensure_clean_source, local_publish_registry, publish_artifact, Destination,
};
use manifest::package_manifest;

use super::{
    source::{clone_at, file_digest, push_source},
    workflow::{
        begin_or_resume_release, cleanup_release, complete_release, validate_release_version,
    },
};

const RELEASE_ACTOR: &str = "package-release-service";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReleaseOptions {
    pub submission: String,
}

pub async fn release(options: PackageReleaseOptions) -> Result<()> {
    let gateway =
        std::env::var("PKG_RELEASE_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let release_token = secret("PKG_RELEASE_TOKEN_FILE")?;
    let publish_token = secret("PKG_RELEASE_PUBLISH_TOKEN_FILE")?;
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(&gateway)?)
        .with_release_token(release_token.clone());
    let submission = client.submission(&options.submission).await?;
    if matches!(
        submission.state,
        WorkflowState::Published | WorkflowState::Closed
    ) {
        let release_id = submission
            .release_id
            .as_deref()
            .context("published submission has no release record")?;
        cleanup_release(&client, release_id, RELEASE_ACTOR).await?;
        println!("{} is already published", submission.submission_id);
        return Ok(());
    }
    if !matches!(
        submission.state,
        WorkflowState::ReadyToRelease | WorkflowState::Publishing
    ) {
        bail!("submission is not ready to release");
    }
    let integration = submission
        .integration
        .as_ref()
        .context("submission has no integration record")?;
    if !integration
        .validation
        .as_ref()
        .is_some_and(|validation| validation.passed())
    {
        bail!("integrated package and consumer checks have not passed");
    }
    let review = submission
        .review
        .as_ref()
        .context("submission has no integration review")?;
    let checkout = client.checkout(&submission.checkout_id).await?;
    if matches!(
        checkout.source_kind,
        SourceKind::ToolBinary | SourceKind::ToolCollection
    ) {
        return super::tool::release_submission(
            &client,
            &submission,
            &release_token,
            &publish_token,
            &gateway,
        )
        .await;
    }
    let definition = client.package_definition(&submission.package).await?;

    let release_root = tempfile::tempdir()?;
    let bundle = release_root.path().join("integration.bundle");
    download_bundle(
        &client.release_bundle_url(&submission.submission_id),
        &release_token,
        &bundle,
    )?;
    let source_archive_digest = checkout
        .workspace_release
        .then(|| file_digest(&bundle))
        .transpose()?;
    let source = release_root.path().join("source");
    let canonical = release_root.path().join("canonical");
    clone_at(&bundle, &source, &integration.integration_commit)?;

    let identity = package_manifest(definition.ecosystem, &source, &definition.name)?;
    if !checkout.initial_release {
        clone_at(&bundle, &canonical, &integration.canonical_commit)?;
        let previous = package_manifest(definition.ecosystem, &canonical, &definition.name)?;
        validate_release_version(
            &client,
            &submission,
            &previous.version,
            &identity.version,
            review.recommended_version,
            RELEASE_ACTOR,
        )
        .await?;
    }
    let tag = format!("v{}", identity.version);
    let artifact = build_artifact(definition.ecosystem, &source, release_root.path())?;
    ensure_clean_source(&source)?;

    if !checkout.workspace_release {
        push_source(
            &source,
            &definition.repository,
            &definition.default_branch,
            &integration.canonical_commit,
            &integration.integration_commit,
            &tag,
        )?;
    }

    let endpoints = RegistryEndpoints::new(&gateway)?;
    let destination = Destination {
        registry: local_publish_registry(definition.ecosystem, &endpoints),
        token: publish_token,
    };
    let release = begin_or_resume_release(
        &client,
        &submission,
        BeginReleaseRequest {
            version: identity.version.to_string(),
            tag: tag.clone(),
            source_commit: integration.integration_commit.clone(),
            artifact_digest: artifact.digest.clone(),
            source_pushed: !checkout.workspace_release,
            source_archive_digest: source_archive_digest.clone(),
            registry: destination.registry.clone(),
            expected_publications: Vec::new(),
            actor: RELEASE_ACTOR.into(),
            idempotency_key: operation_key("release", &submission.submission_id),
        },
    )
    .await?;

    let mut release = release;
    if !release
        .publications
        .iter()
        .any(|publication| publication.registry == destination.registry)
    {
        publish_artifact(
            definition.ecosystem,
            &source,
            &artifact.path,
            &destination,
            release_root.path(),
            checkout.workspace_release,
        )?;
        let publication_key = operation_key(
            "publish",
            &format!("{}:{}", release.release_id, destination.registry),
        );
        release = client
            .record_publication(
                &release.release_id,
                &PublicationRequest {
                    registry: destination.registry,
                    artifact_digest: artifact.digest.clone(),
                    actor: RELEASE_ACTOR.into(),
                    idempotency_key: publication_key,
                },
            )
            .await?;
    }
    let released = complete_release(
        &client,
        &release.release_id,
        RELEASE_ACTOR,
        operation_key("complete", &release.release_id),
    )
    .await?;
    cleanup_release(&client, &released.release_id, RELEASE_ACTOR).await?;
    println!(
        "{}@{} published from {} ({})",
        released.package, released.version, released.source_commit, released.release_id
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::workflow::validate_version_bump;
    use artifact::npm_publish_payload;
    use semver::Version;
    use vm_packages::VersionRecommendation;

    #[test]
    fn reviewed_version_bump_is_a_minimum() {
        assert!(validate_version_bump(
            &Version::new(1, 2, 3),
            &Version::new(1, 2, 4),
            VersionRecommendation::Patch
        )
        .is_ok());
        assert!(validate_version_bump(
            &Version::new(1, 2, 3),
            &Version::new(1, 3, 0),
            VersionRecommendation::Major
        )
        .is_err());
    }

    #[test]
    fn workspace_npm_payload_targets_only_the_private_registry() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"name":"@shared/auth","version":"1.2.3","scripts":{"postpublish":"exit 1"}}"#,
        )
        .unwrap();
        let artifact = directory.path().join("shared-auth-1.2.3.tgz");
        std::fs::write(&artifact, b"private artifact").unwrap();

        let (encoded, payload) =
            npm_publish_payload(directory.path(), &artifact, "http://gateway:8080/npm/").unwrap();

        assert_eq!(encoded, "%40shared%2Fauth");
        assert_eq!(payload["name"], "@shared/auth");
        assert_eq!(payload["dist-tags"]["latest"], "1.2.3");
        assert_eq!(
            payload["versions"]["1.2.3"]["dist"]["tarball"],
            "http://gateway:8080/npm/%40shared%2Fauth/-/shared-auth-1.2.3.tgz"
        );
        assert!(payload["_attachments"]["shared-auth-1.2.3.tgz"]["data"].is_string());
    }
}
