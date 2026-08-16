use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use flate2::{Compression, GzBuilder};
use semver::Version;
use serde::Deserialize;
use tokio_util::io::ReaderStream;
use vm_packages::{
    sha256_hex, sha256_reader, tool_artifact_path, validate_tool_name, BeginReleaseRequest,
    CompleteReleaseRequest, PackageInfrastructureClient, PublicationRequest, PublicationTarget,
    PublishToolArtifact, SubmissionRecord, ToolArtifactRecord, ToolBuild, ToolKind,
    ToolSourceManifest, WorkflowState,
};

use crate::runtime::{operation_key, run_command};

use super::package::{
    cleanup_release, clone_at, download_bundle, push_source, validate_release_version,
};
use super::{file_digest, git, git_text};

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

struct BuiltToolArtifact {
    archive: PathBuf,
    manifest: ToolReleaseManifest,
    request: PublishToolArtifact,
    registry: String,
}

struct ToolArtifactContext<'a> {
    source: &'a Path,
    release_root: &'a Path,
    name: &'a str,
    source_commit: &'a str,
    tag: &'a str,
    submission_id: &'a str,
    gateway: &'a str,
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

/// Release one approved managed tool through the durable package workflow.
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
    let checkout = client.checkout(&submission.checkout_id).await?;
    if !matches!(
        (checkout.source_kind, inventory.definition.kind),
        (vm_packages::SourceKind::ToolBinary, ToolKind::Binary)
            | (
                vm_packages::SourceKind::ToolCollection,
                ToolKind::Collection
            )
    ) {
        bail!("tool checkout kind no longer matches its registered catalog definition");
    }
    let release_root = tempfile::tempdir()?;
    let bundle = release_root.path().join("integration.bundle");
    download_bundle(
        &client.release_bundle_url(&submission.submission_id),
        release_token,
        &bundle,
    )?;
    let source_archive_digest = checkout
        .workspace_release
        .then(|| file_digest(&bundle))
        .transpose()?;
    let source = release_root.path().join("source");
    let canonical = release_root.path().join("canonical");
    clone_at(&bundle, &source, &integration.integration_commit)?;
    clone_at(&bundle, &canonical, &integration.canonical_commit)?;
    let (source_commit, version, previous_version, binary_manifest) =
        match inventory.definition.kind {
            ToolKind::Collection => {
                let identity = collection_identity(&source)?;
                let previous = collection_identity(&canonical)?;
                (
                    identity.source_commit,
                    identity.version,
                    previous.version,
                    None,
                )
            }
            ToolKind::Binary => {
                let identity = binary_identity(&source)?;
                let previous = binary_identity(&canonical)?;
                let version = identity
                    .version
                    .clone()
                    .context("binary tool manifest has no version")?;
                let previous_version = previous
                    .version
                    .context("previous binary tool manifest has no version")?;
                let source_commit = git_text(
                    &source,
                    &["rev-parse", "--verify", "HEAD^{commit}"],
                    "resolve binary tool source commit",
                )?;
                (source_commit, version, previous_version, Some(identity))
            }
        };
    validate_release_version(
        client,
        submission,
        &Version::parse(&previous_version)?,
        &Version::parse(&version)?,
        review.recommended_version,
        RELEASE_ACTOR,
    )
    .await?;
    if source_commit != integration.integration_commit {
        bail!("tool release source does not match the validated integration");
    }

    let tag = format!("v{version}");
    let artifact_context = ToolArtifactContext {
        source: &source,
        release_root: release_root.path(),
        name: &submission.package,
        source_commit: &source_commit,
        tag: &tag,
        submission_id: &submission.submission_id,
        gateway,
    };
    let artifacts = match binary_manifest {
        Some(manifest) => build_binary_artifacts(&artifact_context, &manifest)?,
        None => vec![build_collection_artifact(&artifact_context)?],
    };
    if !checkout.workspace_release {
        push_source(
            &source,
            &inventory.definition.repository,
            &inventory.definition.default_branch,
            &integration.canonical_commit,
            &integration.integration_commit,
            &tag,
        )?;
    }
    let expected_publications = artifacts
        .iter()
        .map(|artifact| PublicationTarget {
            registry: artifact.registry.clone(),
            artifact_digest: artifact.request.artifact_digest.clone(),
        })
        .collect::<Vec<_>>();
    let artifact_digest = if artifacts.len() == 1 {
        artifacts[0].request.artifact_digest.clone()
    } else {
        sha256_hex(
            artifacts
                .iter()
                .map(|artifact| {
                    format!(
                        "{}\0{}\n",
                        artifact.request.target, artifact.request.artifact_digest
                    )
                })
                .collect::<String>(),
        )
    };
    let primary_registry = artifacts[0].registry.clone();
    let mut release = match submission.state {
        WorkflowState::ReadyToRelease => {
            client
                .begin_release(
                    &submission.submission_id,
                    &BeginReleaseRequest {
                        version: version.clone(),
                        tag: tag.clone(),
                        source_commit: source_commit.clone(),
                        artifact_digest: artifact_digest.clone(),
                        source_pushed: !checkout.workspace_release,
                        source_archive_digest: source_archive_digest.clone(),
                        registry: primary_registry.clone(),
                        expected_publications: expected_publications.clone(),
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
            if release.version != version
                || release.tag != tag
                || release.source_commit != source_commit
                || release.artifact_digest != artifact_digest
                || release.registry != primary_registry
                || release.source_pushed != !checkout.workspace_release
                || release.source_archive_digest != source_archive_digest
                || release.expected_publications != expected_publications
            {
                bail!("tool release retry no longer matches its durable record");
            }
            release
        }
        _ => bail!("tool submission is not ready to release"),
    };

    for artifact in artifacts {
        if release.publications.iter().any(|publication| {
            publication.registry == artifact.registry
                && publication.artifact_digest == artifact.request.artifact_digest
        }) {
            continue;
        }
        upload_archive(
            gateway,
            publish_token,
            &artifact.archive,
            &artifact.manifest.name,
            &artifact.request,
        )
        .await?;
        let record = client
            .publish_tool_artifact(&artifact.manifest.name, &artifact.request)
            .await?;
        verify_record(&record, &artifact.manifest, &artifact.request)?;
        release = client
            .record_publication(
                &release.release_id,
                &PublicationRequest {
                    registry: artifact.registry,
                    artifact_digest: artifact.request.artifact_digest.clone(),
                    actor: RELEASE_ACTOR.into(),
                    idempotency_key: operation_key(
                        "tool-publication",
                        &format!("{}:{}", submission.submission_id, artifact.request.target),
                    ),
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

fn build_collection_artifact(context: &ToolArtifactContext<'_>) -> Result<BuiltToolArtifact> {
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

fn build_binary_artifacts(
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
        .map(|build| {
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
        })
        .collect()
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

fn binary_identity(source: &Path) -> Result<ToolSourceManifest> {
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

fn run_isolated(
    arguments: &[String],
    directory: &Path,
    release_root: &Path,
    operation: &str,
) -> Result<()> {
    let (program, arguments) = arguments
        .split_first()
        .context("isolated command cannot be empty")?;
    let mut command = std::process::Command::new("timeout");
    command
        .args(["--signal=TERM", "--kill-after=10s", "30m"])
        .arg(program)
        .args(arguments)
        .current_dir(directory)
        .env_clear()
        .env("HOME", release_root)
        .env("TMPDIR", release_root)
        .env("NPM_CONFIG_OFFLINE", "true")
        .env("CARGO_NET_OFFLINE", "true")
        .env("PIP_NO_INDEX", "1");
    for variable in ["PATH", "CARGO_HOME", "RUSTUP_HOME"] {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    run_command(&mut command, operation)?;
    Ok(())
}

fn native_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux-amd64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}

fn confined_build_archive(source: &Path, relative: &str) -> Result<PathBuf> {
    let source = std::fs::canonicalize(source)?;
    let candidate = source.join(relative);
    let metadata = std::fs::symlink_metadata(&candidate)
        .with_context(|| format!("binary build did not create {relative}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || metadata.len() == 0 {
        bail!("binary build archive must be a non-empty regular file: {relative}");
    }
    let archive = std::fs::canonicalize(&candidate)?;
    if !archive.starts_with(&source) {
        bail!("binary build archive escaped the isolated source directory");
    }
    Ok(archive)
}

fn verify_binary_archive(archive: &Path, build: &ToolBuild) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(File::open(archive)?);
    let mut archive = tar::Archive::new(decoder);
    let expected = build
        .links
        .values()
        .map(PathBuf::from)
        .collect::<std::collections::BTreeSet<_>>();
    let executable = build
        .links
        .iter()
        .filter(|(destination, _)| destination.starts_with(".local/bin/"))
        .map(|(_, source)| PathBuf::from(source))
        .collect::<std::collections::BTreeSet<_>>();
    let mut found = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        validate_archive_path(&path)?;
        if !paths.insert(path.clone()) {
            bail!("binary archive contains duplicate path {}", path.display());
        }
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            bail!(
                "binary archive contains a link or special file: {}",
                path.display()
            );
        }
        if expected.contains(&path) {
            if !entry_type.is_file() || entry.size() == 0 {
                bail!(
                    "linked binary artifact must be a non-empty regular file: {}",
                    path.display()
                );
            }
            if executable.contains(&path) {
                let mode = entry.header().mode()?;
                if mode & 0o111 == 0 {
                    bail!(
                        "linked binary artifact is not executable: {}",
                        path.display()
                    );
                }
                let mut prefix = Vec::new();
                entry.by_ref().take(64).read_to_end(&mut prefix)?;
                validate_executable(&prefix, &build.target, &path)?;
            }
            found.insert(path);
        }
    }
    if found != expected {
        let missing = expected.difference(&found).next().expect("sets differ");
        bail!(
            "binary archive is missing linked artifact {}",
            missing.display()
        );
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        bail!("binary archive contains an unsafe path: {}", path.display());
    }
    Ok(())
}

fn validate_executable(prefix: &[u8], target: &str, path: &Path) -> Result<()> {
    if prefix.starts_with(b"#!") {
        return Ok(());
    }
    if prefix.len() < 20 || !prefix.starts_with(b"\x7fELF") {
        bail!(
            "linked executable is neither an ELF binary nor a script: {}",
            path.display()
        );
    }
    let machine = match prefix[5] {
        1 => u16::from_le_bytes([prefix[18], prefix[19]]),
        2 => u16::from_be_bytes([prefix[18], prefix[19]]),
        _ => bail!(
            "linked ELF executable has invalid byte order: {}",
            path.display()
        ),
    };
    let expected = match target {
        "linux-amd64" => 62,
        "linux-arm64" => 183,
        _ => unreachable!("build target was validated"),
    };
    if machine != expected {
        bail!(
            "linked ELF executable architecture does not match {target}: {}",
            path.display()
        );
    }
    Ok(())
}

fn verify_binary_command(archive: &Path, release_root: &Path, build: &ToolBuild) -> Result<()> {
    let Some(command) = &build.verify else {
        return Ok(());
    };
    if native_target() != Some(build.target.as_str()) {
        bail!(
            "verification command for {} cannot run on this release worker",
            build.target
        );
    }
    let root = release_root.join(format!("verify-{}", build.target));
    std::fs::create_dir(&root)?;
    tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?)).unpack(&root)?;
    let mut command = command.clone();
    let program = std::fs::canonicalize(root.join(&command[0]))?;
    if !program.starts_with(&root) || !std::fs::symlink_metadata(&program)?.is_file() {
        bail!("binary verification executable escaped the extracted archive");
    }
    command[0] = program.to_string_lossy().into_owned();
    run_isolated(
        &command,
        &root,
        release_root,
        &format!("verify binary tool target {}", build.target),
    )
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

    fn binary_repository() -> (tempfile::TempDir, ToolSourceManifest) {
        let directory = tempfile::tempdir().unwrap();
        let target = native_target().unwrap();
        run_command(
            Command::new("git")
                .arg("init")
                .arg("--initial-branch=main")
                .arg(directory.path()),
            "initialize test binary tool",
        )
        .unwrap();
        let manifest = ToolSourceManifest {
            schema: Some(1),
            kind: ToolKind::Binary,
            version: Some("1.2.3".into()),
            builds: vec![ToolBuild {
                target: target.into(),
                command: vec!["./build-tool".into()],
                archive: "dist/release-tool.tar.gz".into(),
                links: BTreeMap::from([(
                    ".local/bin/release-tool".into(),
                    "bin/release-tool".into(),
                )]),
                verify: Some(vec!["bin/release-tool".into(), "--version".into()]),
            }],
        };
        std::fs::write(
            directory.path().join("vm-tool.yaml"),
            serde_yaml_ng::to_string(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("build-tool"),
            "#!/bin/sh\nset -eu\nrm -rf out dist\nmkdir -p out/bin dist\nprintf '#!/bin/sh\\necho 1.2.3\\n' > out/bin/release-tool\nchmod 755 out/bin/release-tool\ntar -C out -czf dist/release-tool.tar.gz bin/release-tool\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                directory.path().join("build-tool"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
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
                "prepare test binary tool",
            )
            .unwrap();
        }
        (directory, manifest)
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

    #[test]
    fn binary_build_is_argument_safe_confined_verified_and_targeted() {
        let (repository, manifest) = binary_repository();
        let release_root = tempfile::tempdir().unwrap();
        let source_commit = git_text(
            repository.path(),
            &["rev-parse", "HEAD"],
            "read test commit",
        )
        .unwrap();
        assert_eq!(binary_identity(repository.path()).unwrap(), manifest);
        let artifacts = build_binary_artifacts(
            &ToolArtifactContext {
                source: repository.path(),
                release_root: release_root.path(),
                name: "release-tool",
                source_commit: &source_commit,
                tag: "v1.2.3",
                submission_id: "submission-1",
                gateway: "http://gateway:8080",
            },
            &manifest,
        )
        .unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].request.target, native_target().unwrap());
        assert_eq!(artifacts[0].request.version, "1.2.3");
        assert!(artifacts[0].request.size_bytes > 0);
        assert_eq!(artifacts[0].request.artifact_digest.len(), 64);
    }

    #[test]
    fn binary_archive_rejects_links_and_special_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("linked.tar.gz");
        let output = File::create(&path).unwrap();
        let gzip = GzBuilder::new().mtime(0).write(output, Compression::best());
        let mut archive = tar::Builder::new(gzip);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_path("bin/release-tool").unwrap();
        header.set_link_name("../../outside").unwrap();
        header.set_size(0);
        header.set_cksum();
        archive.append(&header, io::empty()).unwrap();
        archive.into_inner().unwrap().finish().unwrap();
        let build = ToolBuild {
            target: native_target().unwrap().into(),
            command: vec!["make".into()],
            archive: "dist/release-tool.tar.gz".into(),
            links: BTreeMap::from([(".local/bin/release-tool".into(), "bin/release-tool".into())]),
            verify: None,
        };

        assert!(verify_binary_archive(&path, &build).is_err());
    }
}
