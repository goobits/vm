use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{bail, Context, Result};
use clap::Parser;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use vm_packages::{
    BeginReleaseRequest, CleanupRequest, CompleteReleaseRequest, PackageEcosystem,
    PackageInfrastructureClient, PublicationRequest, RegistryEndpoints, ReleaseRecord,
    VersionRecommendation, WorkflowState,
};

#[derive(Parser)]
#[command(
    name = "pkg-release",
    version,
    about = "Ephemeral deterministic package releaser"
)]
struct Cli {
    #[arg(long)]
    submission: String,
    #[arg(long)]
    push_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageIdentity {
    name: String,
    version: Version,
}

struct BuiltArtifact {
    path: PathBuf,
    digest: String,
}

struct Destination {
    registry: String,
    token: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if !cli.push_source {
        bail!("release publication requires explicit --push-source authorization");
    }
    let gateway =
        std::env::var("PKG_RELEASE_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let release_token = secret("PKG_RELEASE_TOKEN_FILE")?;
    let publish_token = secret("PKG_RELEASE_PUBLISH_TOKEN_FILE")?;
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(&gateway)?)
        .with_release_token(release_token.clone());
    let submission = client.submission(&cli.submission).await?;
    if matches!(
        submission.state,
        WorkflowState::Published | WorkflowState::Closed
    ) {
        let release_id = submission
            .release_id
            .as_deref()
            .context("published submission has no release record")?;
        cleanup_release(&client, release_id).await?;
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
    let definition = client.package_definition(&submission.package).await?;

    let release_root = tempfile::tempdir()?;
    let bundle = release_root.path().join("integration.bundle");
    download_bundle(
        &client.release_bundle_url(&submission.submission_id),
        &release_token,
        &bundle,
    )?;
    let source = release_root.path().join("source");
    let canonical = release_root.path().join("canonical");
    clone_at(&bundle, &source, &integration.integration_commit)?;
    clone_at(&bundle, &canonical, &integration.canonical_commit)?;

    let identity = package_identity(definition.ecosystem, &source, &definition.name)?;
    let previous = package_identity(definition.ecosystem, &canonical, &definition.name)?;
    validate_version_bump(
        &previous.version,
        &identity.version,
        review.recommended_version,
    )?;
    let tag = format!("v{}", identity.version);
    let artifact = build_artifact(definition.ecosystem, &source, release_root.path())?;
    ensure_clean_source(&source)?;

    push_source(
        &source,
        &definition.repository,
        &definition.default_branch,
        &integration.canonical_commit,
        &integration.integration_commit,
        &tag,
    )?;

    let endpoints = RegistryEndpoints::new(&gateway)?;
    let mut destinations = vec![Destination {
        registry: local_publish_registry(definition.ecosystem, &endpoints),
        token: publish_token,
    }];
    if let Some(registry) = definition.ci_registry.clone() {
        destinations.push(Destination {
            registry,
            token: secret("PKG_RELEASE_CI_TOKEN_FILE")?,
        });
    }
    let expected_registries = destinations
        .iter()
        .map(|destination| destination.registry.clone())
        .collect::<Vec<_>>();
    let release = match submission.state {
        WorkflowState::ReadyToRelease => {
            client
                .begin_release(
                    &submission.submission_id,
                    &BeginReleaseRequest {
                        version: identity.version.to_string(),
                        tag: tag.clone(),
                        source_commit: integration.integration_commit.clone(),
                        artifact_digest: artifact.digest.clone(),
                        source_pushed: true,
                        expected_registries,
                        actor: "package-release-service".into(),
                        idempotency_key: operation_key("release", &submission.submission_id),
                    },
                )
                .await?
        }
        WorkflowState::Publishing => {
            let release_id = submission
                .release_id
                .as_deref()
                .context("publishing submission has no release record")?;
            let release = client.release(release_id).await?;
            verify_retry(&release, &identity, &tag, &artifact, &destinations)?;
            release
        }
        _ => unreachable!("release state was validated"),
    };

    let mut release = release;
    for destination in destinations {
        if release
            .publications
            .iter()
            .any(|publication| publication.registry == destination.registry)
        {
            continue;
        }
        publish_artifact(
            definition.ecosystem,
            &source,
            &artifact.path,
            &destination,
            release_root.path(),
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
                    actor: "package-release-service".into(),
                    idempotency_key: publication_key,
                },
            )
            .await?;
    }
    let released = client
        .complete_release(
            &release.release_id,
            &CompleteReleaseRequest {
                actor: "package-release-service".into(),
                idempotency_key: operation_key("complete", &release.release_id),
            },
        )
        .await?;
    cleanup_release(&client, &released.release_id).await?;
    println!(
        "{}@{} published from {} ({})",
        released.package, released.version, released.source_commit, released.release_id
    );
    Ok(())
}

async fn cleanup_release(client: &PackageInfrastructureClient, release_id: &str) -> Result<()> {
    client
        .cleanup_release(
            release_id,
            &CleanupRequest {
                actor: "package-release-service".into(),
                idempotency_key: operation_key("cleanup", release_id),
            },
        )
        .await?;
    Ok(())
}

fn secret(variable: &str) -> Result<String> {
    let path = std::env::var(variable).with_context(|| format!("{variable} is required"))?;
    let value = fs::read_to_string(&path)
        .with_context(|| format!("failed to read secret file {path}"))?
        .trim()
        .to_string();
    if value.is_empty() {
        bail!("secret file configured by {variable} is empty");
    }
    Ok(value)
}

fn download_bundle(url: &str, token: &str, destination: &Path) -> Result<()> {
    run(
        Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--location"])
            .arg("--header")
            .arg(format!("Authorization: Bearer {token}"))
            .arg("--output")
            .arg(destination)
            .arg(url),
        "download validated integration bundle",
    )?;
    Ok(())
}

fn clone_at(bundle: &Path, destination: &Path, commit: &str) -> Result<()> {
    run(
        Command::new("git")
            .arg("clone")
            .arg(bundle)
            .arg(destination),
        "clone release source",
    )?;
    run(
        git()
            .arg("-C")
            .arg(destination)
            .args(["checkout", "--detach", commit]),
        "check out exact release commit",
    )?;
    Ok(())
}

fn package_identity(
    ecosystem: PackageEcosystem,
    source: &Path,
    expected_name: &str,
) -> Result<PackageIdentity> {
    let identity = match ecosystem {
        PackageEcosystem::Npm => npm_identity(source)?,
        PackageEcosystem::Cargo => cargo_identity(source, expected_name)?,
        PackageEcosystem::Python => python_identity(source)?,
    };
    if !package_names_match(ecosystem, &identity.name, expected_name) {
        bail!(
            "package manifest identifies '{}' but the catalog expects '{expected_name}'",
            identity.name
        );
    }
    if !identity.version.pre.is_empty() || !identity.version.build.is_empty() {
        bail!("release versions must be stable semantic versions without build metadata");
    }
    Ok(identity)
}

fn package_names_match(ecosystem: PackageEcosystem, left: &str, right: &str) -> bool {
    if ecosystem != PackageEcosystem::Python {
        return left == right;
    }
    normalize_python_name(left) == normalize_python_name(right)
}

fn normalize_python_name(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut separator = false;
    for character in value.chars() {
        if matches!(character, '-' | '_' | '.') {
            if !separator {
                normalized.push('-');
            }
            separator = true;
        } else {
            normalized.push(character.to_ascii_lowercase());
            separator = false;
        }
    }
    normalized
}

fn npm_identity(source: &Path) -> Result<PackageIdentity> {
    #[derive(Deserialize)]
    struct Manifest {
        name: String,
        version: String,
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(source.join("package.json"))?)?;
    Ok(PackageIdentity {
        name: manifest.name,
        version: Version::parse(&manifest.version)?,
    })
}

fn cargo_identity(source: &Path, expected_name: &str) -> Result<PackageIdentity> {
    #[derive(Deserialize)]
    struct Metadata {
        packages: Vec<CargoPackage>,
    }
    #[derive(Deserialize)]
    struct CargoPackage {
        name: String,
        version: String,
    }
    let output = output(
        Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .arg("--manifest-path")
            .arg(source.join("Cargo.toml")),
        "read Cargo package metadata",
    )?;
    let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
    let package = metadata
        .packages
        .into_iter()
        .find(|package| package.name == expected_name)
        .with_context(|| format!("Cargo workspace has no package named {expected_name}"))?;
    Ok(PackageIdentity {
        name: package.name,
        version: Version::parse(&package.version)?,
    })
}

fn python_identity(source: &Path) -> Result<PackageIdentity> {
    const SCRIPT: &str = r#"import json, pathlib, sys, tomllib
data = tomllib.loads(pathlib.Path(sys.argv[1]).read_text())
project = data.get("project", {})
poetry = data.get("tool", {}).get("poetry", {})
name = project.get("name") or poetry.get("name")
version = project.get("version") or poetry.get("version")
if not name or not version:
    raise SystemExit("pyproject.toml must declare a static name and version")
print(json.dumps({"name": name, "version": version}))"#;
    let output = output(
        Command::new("python3")
            .args(["-c", SCRIPT])
            .arg(source.join("pyproject.toml")),
        "read Python package metadata",
    )?;
    #[derive(Deserialize)]
    struct Identity {
        name: String,
        version: String,
    }
    let identity: Identity = serde_json::from_slice(&output.stdout)?;
    Ok(PackageIdentity {
        name: identity.name,
        version: Version::parse(&identity.version)?,
    })
}

fn validate_version_bump(
    previous: &Version,
    next: &Version,
    recommendation: VersionRecommendation,
) -> Result<()> {
    if next <= previous {
        bail!("release version {next} must be newer than {previous}");
    }
    let actual = if next.major > previous.major {
        VersionRecommendation::Major
    } else if next.minor > previous.minor {
        VersionRecommendation::Minor
    } else {
        VersionRecommendation::Patch
    };
    if bump_rank(actual) < bump_rank(recommendation) {
        bail!("release bump {actual:?} is smaller than the reviewed {recommendation:?} change");
    }
    Ok(())
}

const fn bump_rank(bump: VersionRecommendation) -> u8 {
    match bump {
        VersionRecommendation::Patch => 1,
        VersionRecommendation::Minor => 2,
        VersionRecommendation::Major => 3,
    }
}

fn build_artifact(
    ecosystem: PackageEcosystem,
    source: &Path,
    release_root: &Path,
) -> Result<BuiltArtifact> {
    let path = match ecosystem {
        PackageEcosystem::Npm => {
            let result = output(
                Command::new("npm")
                    .args(["pack", "--json", "--pack-destination"])
                    .arg(release_root)
                    .current_dir(source),
                "build npm release artifact",
            )?;
            let value: serde_json::Value = serde_json::from_slice(&result.stdout)?;
            let filename = value
                .as_array()
                .and_then(|entries| entries.first())
                .and_then(|entry| entry.get("filename"))
                .and_then(serde_json::Value::as_str)
                .context("npm pack did not report its artifact")?;
            if filename.contains('/') || filename.contains("..") {
                bail!("npm returned an unsafe artifact filename");
            }
            release_root.join(filename)
        }
        PackageEcosystem::Cargo => {
            let target = release_root.join("cargo-target");
            run(
                Command::new("cargo")
                    .args(["package", "--no-verify"])
                    .env("CARGO_TARGET_DIR", &target)
                    .current_dir(source),
                "build Cargo release artifact",
            )?;
            single_artifact(&target.join("package"), ".crate")?
        }
        PackageEcosystem::Python => {
            let distribution = release_root.join("python-dist");
            run(
                Command::new("python3")
                    .args(["-m", "build", "--sdist", "--outdir"])
                    .arg(&distribution)
                    .current_dir(source),
                "build Python release artifact",
            )?;
            single_artifact(&distribution, ".tar.gz")?
        }
    };
    let content = fs::read(&path)
        .with_context(|| format!("failed to read built artifact {}", path.display()))?;
    Ok(BuiltArtifact {
        path,
        digest: digest_hex(&content),
    })
}

fn ensure_clean_source(source: &Path) -> Result<()> {
    let status = git_text(source, &["status", "--porcelain"])?;
    if !status.is_empty() {
        bail!("package build modified release source:\n{status}");
    }
    Ok(())
}

fn single_artifact(directory: &Path, suffix: &str) -> Result<PathBuf> {
    let mut matches = fs::read_dir(directory)
        .with_context(|| {
            format!(
                "failed to inspect artifact directory {}",
                directory.display()
            )
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    matches.sort();
    if matches.len() != 1 {
        bail!(
            "expected one {suffix} artifact in {}, found {}",
            directory.display(),
            matches.len()
        );
    }
    Ok(matches.remove(0))
}

fn push_source(
    source: &Path,
    repository: &str,
    branch: &str,
    canonical_commit: &str,
    release_commit: &str,
    tag: &str,
) -> Result<()> {
    run(
        git()
            .arg("-C")
            .arg(source)
            .args(["config", "user.name", "VM Package Release"]),
        "configure release Git identity",
    )?;
    run(
        git()
            .arg("-C")
            .arg(source)
            .args(["config", "user.email", "packages@vm.internal"]),
        "configure release Git identity",
    )?;
    let local_tag = git_text(source, &["rev-parse", &format!("refs/tags/{tag}^{{}}")]).ok();
    match local_tag.as_deref() {
        Some(commit) if commit != release_commit => {
            bail!("release tag {tag} already points to a different commit")
        }
        None => {
            run(
                git()
                    .arg("-C")
                    .arg(source)
                    .args(["tag", "--annotate", tag, "--message"])
                    .arg(format!("Release {tag}"))
                    .arg(release_commit),
                "create release tag",
            )?;
        }
        Some(_) => {}
    }
    let branch_ref = format!("refs/heads/{branch}");
    let remote_branch =
        remote_ref(repository, &branch_ref)?.context("canonical branch is missing")?;
    if remote_branch != canonical_commit && remote_branch != release_commit {
        bail!("canonical branch changed after integration; integrate again before releasing");
    }
    let tag_ref = format!("refs/tags/{tag}");
    let remote_tag = remote_tag_commit(repository, &tag_ref)?;
    if remote_tag
        .as_deref()
        .is_some_and(|commit| commit != release_commit)
    {
        bail!("remote release tag {tag} points to a different commit");
    }
    if remote_branch == release_commit && remote_tag.as_deref() == Some(release_commit) {
        return Ok(());
    }
    let _ = run(
        git()
            .arg("-C")
            .arg(source)
            .args(["remote", "remove", "canonical"]),
        "remove prior release remote",
    );
    run(
        git()
            .arg("-C")
            .arg(source)
            .args(["remote", "add", "canonical", repository]),
        "configure canonical release remote",
    )?;
    let mut command = git();
    command.arg("-C").arg(source).arg("push");
    let pushing_both = remote_branch == canonical_commit && remote_tag.is_none();
    if pushing_both {
        command.arg("--atomic");
    }
    command.arg("canonical");
    if remote_branch == canonical_commit {
        command.arg(format!("{release_commit}:{branch_ref}"));
    }
    if remote_tag.is_none() {
        command.arg(&tag_ref);
    }
    run(&mut command, "push canonical source and release tag")?;
    Ok(())
}

fn remote_ref(repository: &str, reference: &str) -> Result<Option<String>> {
    let output = output(
        git().args(["ls-remote", repository, reference]),
        "inspect canonical Git reference",
    )?;
    Ok(String::from_utf8(output.stdout)?
        .split_whitespace()
        .next()
        .map(str::to_string))
}

fn remote_tag_commit(repository: &str, tag_ref: &str) -> Result<Option<String>> {
    let peeled = format!("{tag_ref}^{{}}");
    let output = output(
        git().args(["ls-remote", repository, tag_ref, &peeled]),
        "inspect canonical release tag",
    )?;
    let text = String::from_utf8(output.stdout)?;
    Ok(text
        .lines()
        .find(|line| line.ends_with("^{}"))
        .or_else(|| text.lines().next())
        .and_then(|line| line.split_whitespace().next())
        .map(str::to_string))
}

fn verify_retry(
    release: &ReleaseRecord,
    identity: &PackageIdentity,
    tag: &str,
    artifact: &BuiltArtifact,
    destinations: &[Destination],
) -> Result<()> {
    let mut expected = destinations
        .iter()
        .map(|destination| destination.registry.as_str())
        .collect::<Vec<_>>();
    expected.sort_unstable();
    if release.version != identity.version.to_string()
        || release.tag != tag
        || release.artifact_digest != artifact.digest
        || release
            .expected_registries
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            != expected
    {
        bail!("retry no longer matches the durable release record");
    }
    Ok(())
}

fn local_publish_registry(ecosystem: PackageEcosystem, endpoints: &RegistryEndpoints) -> String {
    match ecosystem {
        PackageEcosystem::Npm => endpoints.npm(),
        PackageEcosystem::Cargo => endpoints.cargo_index(),
        PackageEcosystem::Python => format!("{}/pypi/upload", endpoints.gateway()),
    }
}

fn publish_artifact(
    ecosystem: PackageEcosystem,
    source: &Path,
    artifact: &Path,
    destination: &Destination,
    release_root: &Path,
) -> Result<()> {
    match ecosystem {
        PackageEcosystem::Npm => {
            let npmrc = release_root.join(format!(
                "npmrc-{}",
                Sha256::digest(destination.registry.as_bytes())
                    .iter()
                    .take(6)
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ));
            let authority = destination
                .registry
                .split_once("://")
                .map(|(_, rest)| rest)
                .context("npm registry must be an HTTP(S) URL")?;
            write_secret_file(
                &npmrc,
                format!(
                    "registry={}\n//{}:_authToken={}\nalways-auth=true\n",
                    destination.registry, authority, destination.token
                )
                .as_bytes(),
            )?;
            run(
                Command::new("npm")
                    .arg("publish")
                    .arg(artifact)
                    .args(["--registry", &destination.registry])
                    .env("NPM_CONFIG_USERCONFIG", npmrc)
                    .current_dir(source),
                "publish npm release",
            )?;
        }
        PackageEcosystem::Cargo => {
            run(
                Command::new("cargo")
                    .args(["publish", "--no-verify", "--registry", "vmrelease"])
                    .arg("--config")
                    .arg(format!(
                        "registries.vmrelease.index=\"{}\"",
                        destination.registry
                    ))
                    .env("CARGO_REGISTRIES_VMRELEASE_TOKEN", &destination.token)
                    .current_dir(source),
                "publish Cargo release",
            )?;
        }
        PackageEcosystem::Python => {
            run(
                Command::new("python3")
                    .args([
                        "-m",
                        "twine",
                        "upload",
                        "--non-interactive",
                        "--repository-url",
                        &destination.registry,
                    ])
                    .arg(artifact)
                    .env("TWINE_USERNAME", "__token__")
                    .env("TWINE_PASSWORD", &destination.token)
                    .current_dir(source),
                "publish Python release",
            )?;
        }
    }
    Ok(())
}

fn write_secret_file(path: &Path, content: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn git_text(repository: &Path, arguments: &[&str]) -> Result<String> {
    let output = output(
        git().arg("-C").arg(repository).args(arguments),
        "inspect Git source",
    )?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn digest_hex(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn operation_key(operation: &str, value: &str) -> String {
    format!("{operation}-{}", &digest_hex(value.as_bytes())[..32])
}

fn git() -> Command {
    let mut command = Command::new("git");
    command.env("GIT_TERMINAL_PROMPT", "0");
    if let Ok(token_file) = std::env::var("PKG_RELEASE_GIT_TOKEN_FILE") {
        command
            .env("GIT_ASKPASS", "pkg-git-askpass")
            .env("PKG_WORK_GIT_TOKEN_FILE", token_file);
    }
    command
}

fn run(command: &mut Command, operation: &str) -> Result<Output> {
    let output = command
        .output()
        .with_context(|| format!("failed to {operation}"))?;
    if output.status.success() {
        return Ok(output);
    }
    bail!(
        "failed to {operation}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn output(command: &mut Command, operation: &str) -> Result<Output> {
    run(command, operation)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
