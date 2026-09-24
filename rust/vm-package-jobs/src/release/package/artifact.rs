use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use vm_packages::{sha256_hex as digest_hex, PackageEcosystem, RegistryEndpoints};

use crate::runtime::{authorization_header, run_command as run};

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
            let locked = ["package-lock.json", "npm-shrinkwrap.json"]
                .iter()
                .any(|file| source.join(file).is_file());
            run(
                Command::new("npm")
                    .args(if locked {
                        vec!["ci", "--ignore-scripts"]
                    } else {
                        vec!["install", "--ignore-scripts", "--package-lock=false"]
                    })
                    .current_dir(source),
                "restore npm release dependencies",
            )?;
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
) -> Result<()> {
    match ecosystem {
        PackageEcosystem::Npm => {
            publish_npm_direct(source, artifact, destination, release_root)?;
        }
        PackageEcosystem::Cargo => {
            super::cargo::publish(source, artifact, destination, release_root)?;
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
    let header = authorization_header(&destination.token)?;
    run(
        Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--request", "PUT"])
            .arg("--header")
            .arg(format!("@{}", header.path().display()))
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

#[cfg(test)]
mod build_tests {
    use super::*;

    #[test]
    #[ignore = "requires npm on PATH; uses only isolated local package fixtures"]
    fn npm_prepack_can_use_declared_build_dependencies() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join("build-dependency")).unwrap();
        fs::write(source.join("package.json"), r#"{"name":"artifact-fixture","version":"1.0.0","scripts":{"prepack":"node -e \"require('build-dependency')\""},"devDependencies":{"build-dependency":"file:./build-dependency"}}"#).unwrap();
        fs::write(
            source.join("build-dependency/package.json"),
            r#"{"name":"build-dependency","version":"1.0.0","main":"index.js"}"#,
        )
        .unwrap();
        fs::write(
            source.join("build-dependency/index.js"),
            "module.exports = true;\n",
        )
        .unwrap();
        fs::write(
            source.join(".npmrc"),
            "registry=http://127.0.0.1:9\nfetch-retries=0\naudit=false\nfund=false\n",
        )
        .unwrap();
        let artifact = build_artifact(PackageEcosystem::Npm, &source, root.path()).unwrap();
        assert!(artifact.path.is_file());
        assert_eq!(
            artifact.digest,
            digest_hex(fs::read(&artifact.path).unwrap())
        );
        assert!(!source.join("package-lock.json").exists());
    }
}
