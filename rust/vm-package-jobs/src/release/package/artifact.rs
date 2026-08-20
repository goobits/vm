use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};
use vm_packages::{sha256_hex as digest_hex, PackageEcosystem, RegistryEndpoints};

use crate::runtime::run_command as run;

use super::super::git_text;

pub(super) struct BuiltArtifact {
    pub(super) path: PathBuf,
    pub(super) digest: String,
}

pub(super) struct Destination {
    pub(super) registry: String,
    pub(super) token: String,
}

pub(super) fn build_artifact(
    ecosystem: PackageEcosystem,
    source: &Path,
    release_root: &Path,
) -> Result<BuiltArtifact> {
    let path = match ecosystem {
        PackageEcosystem::Npm => {
            let result = run(
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

pub(super) fn ensure_clean_source(source: &Path) -> Result<()> {
    let status = git_text(source, &["status", "--porcelain"], "inspect Git source")?;
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

pub(super) fn local_publish_registry(
    ecosystem: PackageEcosystem,
    endpoints: &RegistryEndpoints,
) -> String {
    match ecosystem {
        PackageEcosystem::Npm => endpoints.npm(),
        PackageEcosystem::Cargo => endpoints.cargo_index(),
        PackageEcosystem::Python => format!("{}/pypi/upload", endpoints.gateway()),
    }
}

pub(super) fn publish_artifact(
    ecosystem: PackageEcosystem,
    source: &Path,
    artifact: &Path,
    destination: &Destination,
    release_root: &Path,
    workspace_release: bool,
) -> Result<()> {
    match ecosystem {
        PackageEcosystem::Npm if workspace_release => {
            publish_npm_direct(source, artifact, destination, release_root)?;
        }
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

fn publish_npm_direct(
    source: &Path,
    artifact: &Path,
    destination: &Destination,
    release_root: &Path,
) -> Result<()> {
    let (encoded_name, payload) = npm_publish_payload(source, artifact, &destination.registry)?;
    let payload_path = release_root.join("npm-publish.json");
    write_secret_file(&payload_path, &serde_json::to_vec(&payload)?)?;
    let registry = format!("{}/", destination.registry.trim_end_matches('/'));
    run(
        Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--request", "PUT"])
            .arg("--header")
            .arg(format!("Authorization: Bearer {}", destination.token))
            .args([
                "--header",
                "Content-Type: application/json",
                "--data-binary",
            ])
            .arg(format!("@{}", payload_path.display()))
            .arg(format!("{registry}{encoded_name}")),
        "publish npm release directly to the private registry",
    )?;
    Ok(())
}

pub(super) fn npm_publish_payload(
    source: &Path,
    artifact: &Path,
    registry: &str,
) -> Result<(String, serde_json::Value)> {
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(source.join("package.json"))?)?;
    let name = manifest["name"]
        .as_str()
        .context("package.json name is missing")?
        .to_string();
    let version = manifest["version"]
        .as_str()
        .context("package.json version is missing")?
        .to_string();
    let filename = artifact
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.ends_with(".tgz") && !name.contains(['/', '\\']))
        .context("npm artifact filename is invalid")?;
    let encoded_name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
    let registry = format!("{}/", registry.trim_end_matches('/'));
    let tarball = format!("{registry}{encoded_name}/-/{filename}");
    manifest["dist"] = serde_json::json!({"tarball": tarball});
    let content = fs::read(artifact)?;
    Ok((
        encoded_name,
        serde_json::json!({
            "_id": name,
            "name": name,
            "dist-tags": {"latest": version},
            "versions": {version.clone(): manifest},
            "_attachments": {
                filename: {
                    "content_type": "application/octet-stream",
                    "data": general_purpose::STANDARD.encode(&content),
                    "length": content.len()
                }
            }
        }),
    ))
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
