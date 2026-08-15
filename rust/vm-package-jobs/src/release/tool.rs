use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use flate2::{Compression, GzBuilder};
use semver::Version;
use serde::Deserialize;
use tokio_util::io::ReaderStream;
use vm_packages::{
    sha256_reader, tool_artifact_path, validate_tool_name, BeginReleaseRequest,
    CompleteReleaseRequest, PackageInfrastructureClient, PublicationRequest, PublishToolArtifact,
    SubmissionRecord, ToolArtifactRecord, ToolKind, WorkflowState,
};

use crate::runtime::{operation_key, run_command};

use super::package::{
    cleanup_release, clone_at, download_bundle, push_source, validate_release_version,
};
use super::{git, git_text};

const TOOL_TARGET: &str = "any";
const RELEASE_ACTOR: &str = "tool-release-service";

struct BuiltCollection {
    archive: PathBuf,
    source_commit: String,
    version: String,
}

struct CollectionIdentity {
    source_commit: String,
    version: String,
}

/// Inputs shared by tool publishers regardless of how their source archive was built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolReleaseManifest {
    pub name: String,
    pub version: String,
    pub target: String,
    pub links: BTreeMap<String, String>,
    pub source_commit: String,
    pub tag: String,
    pub actor: String,
    pub idempotency_key: String,
}

/// Release one approved tool-collection submission through the durable package workflow.
pub(super) async fn release_submission(
    client: &PackageInfrastructureClient,
    submission: &SubmissionRecord,
    release_token: &str,
    publish_token: &str,
    gateway: &str,
) -> Result<()> {
    let integration = submission
        .integration
        .as_ref()
        .context("tool submission has no integration record")?;
    let review = submission
        .review
        .as_ref()
        .context("tool submission has no integration review")?;
    let inventory = client.tool(&submission.package).await?;
    if inventory.definition.kind != ToolKind::Collection {
        bail!("managed tool checkout is not a collection");
    }

    let release_root = tempfile::tempdir()?;
    let bundle = release_root.path().join("integration.bundle");
    download_bundle(
        &client.release_bundle_url(&submission.submission_id),
        release_token,
        &bundle,
    )?;
    let source = release_root.path().join("source");
    let canonical = release_root.path().join("canonical");
    clone_at(&bundle, &source, &integration.integration_commit)?;
    clone_at(&bundle, &canonical, &integration.canonical_commit)?;
    let identity = collection_identity(&source)?;
    let previous = collection_identity(&canonical)?;
    validate_release_version(
        client,
        submission,
        &Version::parse(&previous.version)?,
        &Version::parse(&identity.version)?,
        review.recommended_version,
        RELEASE_ACTOR,
    )
    .await?;
    if identity.source_commit != integration.integration_commit {
        bail!("tool release source does not match the validated integration");
    }

    let built = build_collection(&source, &release_root.path().join("tool.tar.gz"))?;
    let tag = format!("v{}", built.version);
    push_source(
        &source,
        &inventory.definition.repository,
        &inventory.definition.default_branch,
        &integration.canonical_commit,
        &integration.integration_commit,
        &tag,
    )?;
    let manifest = ToolReleaseManifest {
        name: submission.package.clone(),
        version: built.version,
        target: TOOL_TARGET.into(),
        links: collection_links(),
        source_commit: built.source_commit,
        tag,
        actor: RELEASE_ACTOR.into(),
        idempotency_key: operation_key("tool-workflow", &submission.submission_id),
    };
    let request = publication_request(&built.archive, manifest.clone())?;
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
    let mut release = match submission.state {
        WorkflowState::ReadyToRelease => {
            client
                .begin_release(
                    &submission.submission_id,
                    &BeginReleaseRequest {
                        version: request.version.clone(),
                        tag: request.tag.clone(),
                        source_commit: request.source_commit.clone(),
                        artifact_digest: request.artifact_digest.clone(),
                        source_pushed: true,
                        source_archive_digest: None,
                        registry: registry.clone(),
                        expected_publications: Vec::new(),
                        actor: RELEASE_ACTOR.into(),
                        idempotency_key: operation_key("tool-release", &submission.submission_id),
                    },
                )
                .await?
        }
        WorkflowState::Publishing => {
            let release_id = submission
                .release_id
                .as_deref()
                .context("publishing tool submission has no release record")?;
            let release = client.release(release_id).await?;
            if release.version != request.version
                || release.tag != request.tag
                || release.source_commit != request.source_commit
                || release.artifact_digest != request.artifact_digest
                || release.registry != registry
            {
                bail!("tool release retry no longer matches its durable record");
            }
            release
        }
        _ => bail!("tool submission is not ready to release"),
    };

    if !release
        .publications
        .iter()
        .any(|publication| publication.registry == registry)
    {
        upload_archive(
            gateway,
            publish_token,
            &built.archive,
            &manifest.name,
            &request,
        )
        .await?;
        let record = client
            .publish_tool_artifact(&manifest.name, &request)
            .await?;
        verify_record(&record, &manifest, &request)?;
        release = client
            .record_publication(
                &release.release_id,
                &PublicationRequest {
                    registry,
                    artifact_digest: request.artifact_digest,
                    actor: RELEASE_ACTOR.into(),
                    idempotency_key: operation_key("tool-publication", &submission.submission_id),
                },
            )
            .await?;
    }
    let released = client
        .complete_release(
            &release.release_id,
            &CompleteReleaseRequest {
                actor: RELEASE_ACTOR.into(),
                idempotency_key: operation_key("tool-complete", &submission.submission_id),
            },
        )
        .await?;
    cleanup_release(client, &released.release_id).await?;
    println!(
        "{} {} published from {} ({})",
        released.package, released.version, released.source_commit, released.release_id
    );
    Ok(())
}

fn build_collection(source: &Path, archive: &Path) -> Result<BuiltCollection> {
    let identity = collection_identity(source)?;
    create_archive(source, archive)?;
    Ok(BuiltCollection {
        archive: archive.to_path_buf(),
        source_commit: identity.source_commit,
        version: identity.version,
    })
}

fn collection_identity(source: &Path) -> Result<CollectionIdentity> {
    #[derive(Deserialize)]
    struct CollectionManifest {
        version: String,
    }

    let manifest: CollectionManifest = serde_json::from_str(&git_text(
        source,
        &["show", "HEAD:package.json"],
        "read collection release manifest",
    )?)
    .context("collection package.json is invalid")?;
    let version = Version::parse(&manifest.version)
        .context("collection package.json version must be semantic")?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        bail!("collection release version must be stable without build metadata");
    }
    let source_commit = git_text(
        source,
        &["rev-parse", "--verify", "HEAD^{commit}"],
        "resolve collection source commit",
    )?;
    Ok(CollectionIdentity {
        source_commit,
        version: version.to_string(),
    })
}

fn create_archive(source: &Path, destination: &Path) -> Result<()> {
    let tar_path = destination.with_extension("tar");
    run_command(
        git()
            .arg("-C")
            .arg(source)
            .args(["archive", "--format=tar", "--prefix=skills/", "--output"])
            .arg(&tar_path)
            .arg("HEAD"),
        "archive collection source",
    )?;
    let mut tar = File::open(&tar_path)
        .with_context(|| format!("read collection archive {}", tar_path.display()))?;
    let output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .with_context(|| format!("create collection archive {}", destination.display()))?;
    let mut gzip = GzBuilder::new().mtime(0).write(output, Compression::best());
    io::copy(&mut tar, &mut gzip)?;
    gzip.finish()?.sync_all()?;
    std::fs::remove_file(&tar_path)?;
    Ok(())
}

fn collection_links() -> BTreeMap<String, String> {
    [
        ".agents/skills",
        ".claude/skills",
        ".codex/skills",
        ".gemini/skills",
        ".config/antigravity/skills",
    ]
    .into_iter()
    .map(|destination| (destination.into(), "skills".into()))
    .collect()
}

async fn upload_archive(
    gateway: &str,
    publish_token: &str,
    archive: &Path,
    name: &str,
    request: &PublishToolArtifact,
) -> Result<()> {
    let artifact_path = tool_artifact_path(
        name,
        &request.version,
        &request.target,
        &request.artifact_digest,
    );
    let url = format!("{}{}", gateway.trim_end_matches('/'), artifact_path);
    let file = tokio::fs::File::open(archive)
        .await
        .with_context(|| format!("open tool artifact {}", archive.display()))?;
    let response = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(600))
        .build()?
        .put(&url)
        .bearer_auth(publish_token)
        .header(reqwest::header::CONTENT_LENGTH, request.size_bytes)
        .body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
        .send()
        .await
        .with_context(|| format!("upload tool artifact to {url}"))?;
    response
        .error_for_status_ref()
        .with_context(|| format!("tool registry rejected PUT {url}"))?;
    let received = response
        .headers()
        .get("x-checksum-sha256")
        .context("tool registry omitted its checksum")?
        .to_str()
        .context("tool registry returned an invalid checksum")?;
    if received != request.artifact_digest {
        bail!("tool registry checksum does not match the uploaded artifact");
    }
    Ok(())
}

/// Verify an archive once and derive the exact workflow metadata from its bytes.
pub fn publication_request(
    archive: &Path,
    manifest: ToolReleaseManifest,
) -> Result<PublishToolArtifact> {
    validate_tool_name(&manifest.name)?;
    let metadata = std::fs::metadata(archive)
        .with_context(|| format!("read tool archive metadata from {}", archive.display()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        bail!("tool archive must be a non-empty regular file");
    }
    let file = std::fs::File::open(archive)
        .with_context(|| format!("read tool archive {}", archive.display()))?;
    let (artifact_digest, size_bytes) = sha256_reader(BufReader::new(file))?;
    let request = PublishToolArtifact {
        version: manifest.version,
        target: manifest.target,
        artifact_digest,
        size_bytes,
        links: manifest.links,
        source_commit: manifest.source_commit,
        tag: manifest.tag,
        actor: manifest.actor,
        idempotency_key: manifest.idempotency_key,
    };
    request.validate()?;
    Ok(request)
}

pub fn verify_record(
    record: &ToolArtifactRecord,
    manifest: &ToolReleaseManifest,
    request: &PublishToolArtifact,
) -> Result<()> {
    if record.tool != manifest.name
        || record.version != request.version
        || record.target != request.target
        || record.artifact_digest != request.artifact_digest
        || record.size_bytes != request.size_bytes
        || record.links != request.links
        || record.source_commit != request.source_commit
        || record.tag != request.tag
    {
        bail!("existing tool publication does not match this release attempt");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn collection_repository() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        run_command(
            Command::new("git")
                .arg("init")
                .arg("--initial-branch=main")
                .arg(directory.path()),
            "initialize test collection",
        )
        .unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"name":"agent-skills","version":"0.6.0"}"#,
        )
        .unwrap();
        std::fs::create_dir(directory.path().join("x-test")).unwrap();
        std::fs::write(directory.path().join("x-test/SKILL.md"), "# Test skill\n").unwrap();
        for arguments in [
            vec!["config", "user.name", "VM Tests"],
            vec!["config", "user.email", "vm-tests@example.invalid"],
            vec!["add", "--all"],
            vec!["-c", "commit.gpgsign=false", "commit", "-m", "release"],
        ] {
            run_command(
                Command::new("git")
                    .arg("-C")
                    .arg(directory.path())
                    .args(arguments),
                "prepare test collection",
            )
            .unwrap();
        }
        directory
    }

    #[test]
    fn derives_valid_publication_metadata_from_one_archive() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("tool.tar.gz");
        std::fs::write(&archive, b"archive").unwrap();
        let request = publication_request(
            &archive,
            ToolReleaseManifest {
                name: "codex".into(),
                version: "1.0.0".into(),
                target: "linux-arm64".into(),
                links: BTreeMap::from([(".local/bin/codex".into(), "bin/codex".into())]),
                source_commit: "a".repeat(40),
                tag: "v1.0.0".into(),
                actor: "release-service".into(),
                idempotency_key: "codex-1.0.0-linux-arm64".into(),
            },
        )
        .unwrap();
        assert_eq!(request.size_bytes, 7);
        assert_eq!(request.artifact_digest.len(), 64);
    }

    #[test]
    fn collection_archive_is_deterministic_and_activates_for_supported_agents() {
        let repository = collection_repository();
        let first_directory = tempfile::tempdir().unwrap();
        let second_directory = tempfile::tempdir().unwrap();
        let first = build_collection(
            repository.path(),
            &first_directory.path().join("one.tar.gz"),
        )
        .unwrap();
        let second = build_collection(
            repository.path(),
            &second_directory.path().join("two.tar.gz"),
        )
        .unwrap();

        assert_eq!(first.version, "0.6.0");
        assert_eq!(first.source_commit, second.source_commit);
        assert_eq!(first.source_commit.len(), 40);
        assert_eq!(
            std::fs::read(first.archive).unwrap(),
            std::fs::read(second.archive).unwrap()
        );
        assert_eq!(collection_links().len(), 5);
        assert_eq!(collection_links()[".agents/skills"], "skills");

        let archive = File::open(first_directory.path().join("one.tar.gz")).unwrap();
        let decoder = flate2::read::GzDecoder::new(archive);
        let mut archive = tar::Archive::new(decoder);
        let paths = archive
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap().path().unwrap().into_owned())
            .collect::<Vec<_>>();
        assert!(paths.contains(&PathBuf::from("skills/package.json")));
        assert!(paths.contains(&PathBuf::from("skills/x-test/SKILL.md")));
    }
}
