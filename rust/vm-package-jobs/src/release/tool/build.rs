use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use vm_packages::{
    sha256_hex, validate_managed_id, CompleteToolBuildRequest, PackageInfrastructureClient,
    SubmissionRecord, ToolBuildArtifact, ToolKind, ToolSourceManifest, WorkflowState,
};

use crate::runtime::{download_bundle, operation_key, run_command};

use super::artifact::{
    binary_identity, build_binary_artifacts, BuiltToolArtifact, ToolArtifactContext,
};
use super::publication::publication_request;
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

pub(super) fn run_isolated(
    arguments: &[String],
    directory: &Path,
    release_root: &Path,
    operation: &str,
) -> Result<()> {
    let (program, arguments) = arguments
        .split_first()
        .context("isolated command cannot be empty")?;
    let mut command = Command::new("timeout");
    let sandbox_home = release_root.join("untrusted");
    let sandbox_home = if sandbox_home.is_dir() {
        sandbox_home.as_path()
    } else {
        release_root
    };
    let package_gateway = std::env::var("PKG_BUILD_PACKAGE_GATEWAY")
        .unwrap_or_else(|_| "http://build-edge:3080".into());
    let package_gateway = package_gateway.trim_end_matches('/');
    let cargo_home = sandbox_home.join("cargo-home");
    std::fs::create_dir_all(&cargo_home).context("create isolated Cargo home")?;
    let cargo_config = cargo_home.join("config.toml");
    if !cargo_config.is_file() {
        std::fs::write(&cargo_config, cargo_source_config(package_gateway)?)
            .context("write isolated Cargo source configuration")?;
    }
    command
        .args(["--signal=TERM", "--kill-after=10s", "30m"])
        .arg(program)
        .args(arguments)
        .current_dir(directory)
        .env_clear()
        .env("HOME", sandbox_home)
        .env("TMPDIR", sandbox_home)
        .env("XDG_CACHE_HOME", sandbox_home.join("xdg-cache"))
        .env("CARGO_HOME", cargo_home)
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

pub(super) fn prepare_isolated_package_configuration(release_root: &Path) -> Result<()> {
    let package_gateway = std::env::var("PKG_BUILD_PACKAGE_GATEWAY")
        .unwrap_or_else(|_| "http://build-edge:3080".into());
    let cargo_home = release_root.join("untrusted/cargo-home");
    std::fs::create_dir_all(&cargo_home).context("create isolated Cargo home")?;
    std::fs::write(
        cargo_home.join("config.toml"),
        cargo_source_config(package_gateway.trim_end_matches('/'))?,
    )
    .context("write isolated Cargo source configuration")?;
    Ok(())
}

pub(super) fn cargo_source_config(package_gateway: &str) -> Result<String> {
    let gateway = url::Url::parse(package_gateway).context("parse package build gateway")?;
    if !matches!(gateway.scheme(), "http" | "https") {
        bail!("package build gateway must use HTTP(S)");
    }
    let registry = format!(
        "sparse+{}/cargo/index/",
        gateway.as_str().trim_end_matches('/')
    );
    Ok(format!(
        "[source.crates-io]\nreplace-with = \"vm\"\n\n[source.vm]\nregistry = \"{registry}\"\n"
    ))
}

pub(super) fn native_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux-amd64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}
