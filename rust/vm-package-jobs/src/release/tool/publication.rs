use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio_util::io::ReaderStream;
use vm_packages::{
    sha256_reader, tool_artifact_path, validate_tool_name, PublishToolArtifact, ToolArtifactRecord,
};

use super::artifact::ToolReleaseManifest;

pub(super) async fn upload_archive(
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

pub(super) fn publication_request(
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

pub(super) fn verify_record(
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
