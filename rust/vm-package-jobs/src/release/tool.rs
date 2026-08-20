use std::collections::BTreeMap;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use semver::Version;
use tokio_util::io::ReaderStream;
use vm_packages::{
    sha256_hex, sha256_reader, tool_artifact_path, validate_tool_name, BeginReleaseRequest,
    PackageInfrastructureClient, PublicationRequest, PublicationTarget, PublishToolArtifact,
    SubmissionRecord, ToolArtifactRecord, ToolBuildRecord, ToolKind, ToolSourceManifest,
};

use crate::runtime::{download_bundle, operation_key, run_command};

use super::{
    git_text,
    source::{clone_at, file_digest, push_source},
    workflow::{
        begin_or_resume_release, cleanup_release, complete_release, validate_release_version,
    },
};

const TOOL_TARGET: &str = "any";
const RELEASE_ACTOR: &str = "tool-release-service";

mod archive;
mod build;

pub use build::build_submission;

use archive::{
    build_collection, collection_identity, collection_links, confined_build_archive,
    verify_binary_archive, verify_binary_command,
};

pub(crate) struct BuiltToolArtifact {
    pub(crate) archive: PathBuf,
    pub(crate) manifest: ToolReleaseManifest,
    pub(crate) request: PublishToolArtifact,
    pub(crate) registry: String,
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
    let (source_commit, version, binary_manifest) = match inventory.definition.kind {
        ToolKind::Collection => {
            let identity = collection_identity(&source)?;
            (identity.source_commit, identity.version, None)
        }
        ToolKind::Binary => {
            let identity = binary_identity(&source)?;
            let version = identity
                .version
                .clone()
                .context("binary tool manifest has no version")?;
            let source_commit = git_text(
                &source,
                &["rev-parse", "--verify", "HEAD^{commit}"],
                "resolve binary tool source commit",
            )?;
            (source_commit, version, Some(identity))
        }
    };
    if !checkout.initial_release {
        clone_at(&bundle, &canonical, &integration.canonical_commit)?;
        let previous_version = match inventory.definition.kind {
            ToolKind::Collection => collection_identity(&canonical)?.version,
            ToolKind::Binary => binary_identity(&canonical)?
                .version
                .context("previous binary tool manifest has no version")?,
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
    }
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
        Some(manifest) => {
            let build = client.tool_build(&submission.submission_id).await?;
            let staging_root = PathBuf::from(
                std::env::var_os("PKG_RELEASE_BUILD_ROOT")
                    .context("PKG_RELEASE_BUILD_ROOT is required for binary tools")?,
            );
            staged_binary_artifacts(&artifact_context, &manifest, &build, &staging_root)?
        }
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
    let mut release = begin_or_resume_release(
        client,
        submission,
        BeginReleaseRequest {
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
    .await?;

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
    let released = complete_release(
        client,
        &release.release_id,
        RELEASE_ACTOR,
        operation_key("tool-complete", &submission.submission_id),
    )
    .await?;
    cleanup_release(client, &released.release_id, RELEASE_ACTOR).await?;
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

fn staged_binary_artifacts(
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

pub(super) fn run_isolated(
    arguments: &[String],
    directory: &Path,
    release_root: &Path,
    operation: &str,
) -> Result<()> {
    let (program, arguments) = arguments
        .split_first()
        .context("isolated command cannot be empty")?;
    let mut command = std::process::Command::new("timeout");
    let sandbox_home = release_root.join("untrusted");
    let sandbox_home = if sandbox_home.is_dir() {
        sandbox_home.as_path()
    } else {
        release_root
    };
    let package_gateway = std::env::var("PKG_BUILD_PACKAGE_GATEWAY")
        .unwrap_or_else(|_| "http://build-edge:3080".into());
    let package_gateway = package_gateway.trim_end_matches('/');
    command
        .args(["--signal=TERM", "--kill-after=10s", "30m"])
        .arg(program)
        .args(arguments)
        .current_dir(directory)
        .env_clear()
        .env("HOME", sandbox_home)
        .env("TMPDIR", sandbox_home)
        .env("XDG_CACHE_HOME", sandbox_home.join("xdg-cache"))
        .env("CARGO_HOME", sandbox_home.join("cargo-home"))
        .env("CARGO_TARGET_DIR", sandbox_home.join("cargo-target"))
        .env("npm_config_cache", sandbox_home.join("npm-cache"))
        .env("PIP_CACHE_DIR", sandbox_home.join("pip-cache"))
        .env("NPM_CONFIG_REGISTRY", format!("{package_gateway}/npm/"))
        .env("PIP_INDEX_URL", format!("{package_gateway}/pypi/simple/"))
        .env("UV_INDEX_URL", format!("{package_gateway}/pypi/simple/"))
        .env(
            "CARGO_REGISTRIES_VM_INDEX",
            format!("sparse+{package_gateway}/cargo/index/"),
        )
        .env("CARGO_SOURCE_CRATES_IO_REPLACE_WITH", "vm")
        .env(
            "CARGO_SOURCE_VM_REGISTRY",
            format!("sparse+{package_gateway}/cargo/index/"),
        );
    for variable in ["PATH", "RUSTUP_HOME"] {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    #[cfg(unix)]
    if let Some(uid) = std::env::var_os("PKG_BUILD_UID") {
        use std::os::unix::process::CommandExt;

        let uid = uid
            .to_string_lossy()
            .parse::<u32>()
            .context("PKG_BUILD_UID must be a numeric user ID")?;
        let gid = std::env::var("PKG_BUILD_GID")
            .context("PKG_BUILD_GID is required")?
            .parse::<u32>()
            .context("PKG_BUILD_GID must be a numeric group ID")?;
        command.uid(uid).gid(gid);
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
    use flate2::{Compression, GzBuilder};
    use std::fs::File;
    use std::io;
    use std::process::Command;
    use vm_packages::ToolBuild;

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
        let target = native_target().unwrap_or("linux-arm64");
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
        if native_target().is_none() {
            return;
        }
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
            target: native_target().unwrap_or("linux-arm64").into(),
            command: vec!["make".into()],
            archive: "dist/release-tool.tar.gz".into(),
            links: BTreeMap::from([(".local/bin/release-tool".into(), "bin/release-tool".into())]),
            verify: None,
        };

        assert!(verify_binary_archive(&path, &build).is_err());
    }
}
