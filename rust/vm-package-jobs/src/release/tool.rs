use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use semver::Version;
use vm_packages::{
    sha256_hex, BeginReleaseRequest, PackageInfrastructureClient, PublicationRequest,
    PublicationTarget, SubmissionRecord, ToolKind,
};

use crate::runtime::{download_bundle, operation_key};

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
mod artifact;
mod build;
mod build_workspace;
mod dependencies;
mod publication;

pub use build::{build_submission, prepare_build_work_root};

use archive::collection_identity;
use artifact::{
    binary_identity, build_collection_artifact, staged_binary_artifacts, ToolArtifactContext,
};
use publication::{upload_archive, verify_record};

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
    tracing::info!(
        operation = "release",
        submission_id = %submission.submission_id,
        release_id = %released.release_id,
        package = %released.package,
        version = %released.version,
        source_commit = %released.source_commit,
        outcome = "published",
        "tool release completed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::archive::{
        build_collection, collection_links, is_portable_script, verify_binary_archive,
    };
    use super::artifact::{
        binary_identity, build_binary_artifacts, ToolArtifactContext, ToolReleaseManifest,
    };
    use super::build::{
        cargo_source_config, native_target, prepare_isolated_package_configuration,
    };
    use super::publication::publication_request;
    use super::*;
    use crate::runtime::run_command;
    use flate2::{Compression, GzBuilder};
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::io;
    use std::process::Command;
    use vm_packages::{ToolBuild, ToolSourceManifest};

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
            schema: 1,
            kind: ToolKind::Binary,
            version: Some("1.2.3".into()),
            build_sources: Vec::new(),
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
    fn isolated_cargo_configuration_replaces_public_crates_io() {
        let config = cargo_source_config("http://build-edge:3080").unwrap();
        assert!(config.contains("[source.crates-io]"));
        assert!(config.contains("replace-with = \"vm\""));
        assert!(config.contains("registry = \"sparse+http://build-edge:3080/cargo/index/\""));
        assert!(cargo_source_config("file:///tmp/registry").is_err());

        let release_root = tempfile::tempdir().unwrap();
        prepare_isolated_package_configuration(release_root.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(release_root.path().join("untrusted/cargo-home/config.toml"))
                .unwrap(),
            config
        );
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

    #[test]
    fn cross_target_verification_only_treats_shebang_programs_as_portable() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("tool-script");
        let binary = directory.path().join("tool-binary");
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(&binary, b"\x7fELFnot-a-script").unwrap();

        assert!(is_portable_script(&script).unwrap());
        assert!(!is_portable_script(&binary).unwrap());
    }
}
