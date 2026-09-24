use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use super::artifact::Destination;
use crate::runtime::{authorization_header, run_command as run};

pub(super) fn publish(
    source: &Path,
    artifact: &Path,
    destination: &Destination,
    release_root: &Path,
) -> Result<()> {
    let output = run(
        Command::new("cargo")
            .args([
                "metadata",
                "--offline",
                "--no-deps",
                "--format-version",
                "1",
            ])
            .current_dir(source),
        "read Cargo publication metadata",
    )?;
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let filename = artifact
        .file_name()
        .and_then(|name| name.to_str())
        .context("crate filename is missing")?;
    let package = metadata["packages"]
        .as_array()
        .context("Cargo metadata has no packages")?
        .iter()
        .find(|package| {
            package["name"]
                .as_str()
                .zip(package["version"].as_str())
                .is_some_and(|(name, version)| filename == format!("{name}-{version}.crate"))
        })
        .context("built crate does not match a package in the source metadata")?;
    let payload = upload_payload(package, &fs::read(artifact)?)?;
    let body = release_root.join("cargo-publish.bin");
    fs::write(&body, payload)?;
    let header = authorization_header(&destination.token)?;
    let api = destination
        .registry
        .strip_prefix("sparse+")
        .and_then(|index| index.strip_suffix("/index/"))
        .context("private Cargo publication requires its managed sparse index")?;
    run(
        Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--request",
                "PUT",
                "--header",
            ])
            .arg(format!("@{}", header.path().display()))
            .args([
                "--header",
                "Content-Type: application/octet-stream",
                "--data-binary",
            ])
            .arg(format!("@{}", body.display()))
            .arg(format!("{api}/api/v1/crates/new")),
        "publish retained Cargo artifact",
    )?;
    Ok(())
}

fn upload_payload(package: &Value, artifact: &[u8]) -> Result<Vec<u8>> {
    let dependencies = package["dependencies"].as_array().context("Cargo metadata has no dependencies")?
        .iter().map(|dependency| json!({
            "name": dependency["name"],
            "version_req": dependency["req"],
            "features": dependency["features"],
            "optional": dependency["optional"],
            "default_features": dependency["uses_default_features"],
            "target": dependency["target"],
            "kind": dependency["kind"].as_str().unwrap_or("normal"),
            "registry": dependency["registry"].as_str().unwrap_or("https://github.com/rust-lang/crates.io-index"),
            "explicit_name_in_toml": dependency["rename"],
        })).collect::<Vec<_>>();
    let metadata = serde_json::to_vec(&json!({
        "name": package["name"], "vers": package["version"],
        "deps": dependencies, "features": package["features"],
        "links": package["links"], "rust_version": package["rust_version"],
    }))?;
    let mut body = Vec::with_capacity(8 + metadata.len() + artifact.len());
    body.extend_from_slice(&u32::try_from(metadata.len())?.to_le_bytes());
    body.extend_from_slice(&metadata);
    body.extend_from_slice(&u32::try_from(artifact.len())?.to_le_bytes());
    body.extend_from_slice(artifact);
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_preserves_built_bytes_and_converts_cargo_metadata() {
        let package = json!({"name":"shared", "version":"1.2.0", "features":{"serde":["dep:codec"]}, "links":"native", "rust_version":"1.80", "dependencies":[{
            "name":"serde", "req":"^1", "rename":"codec", "features":["derive"], "optional":true,
            "uses_default_features":false, "kind":null, "target":"cfg(unix)", "registry":null
        }]});
        let body = upload_payload(&package, b"exact retained crate bytes").unwrap();
        let length = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
        let metadata: Value = serde_json::from_slice(&body[4..4 + length]).unwrap();
        assert_eq!(metadata["vers"], "1.2.0");
        assert_eq!(metadata["deps"][0]["version_req"], "^1");
        assert_eq!(metadata["deps"][0]["explicit_name_in_toml"], "codec");
        assert_eq!(metadata["deps"][0]["default_features"], false);
        assert_eq!(
            metadata["deps"][0]["registry"],
            "https://github.com/rust-lang/crates.io-index"
        );
        assert_eq!(&body[8 + length..], b"exact retained crate bytes");
        assert_eq!(
            u32::from_le_bytes(body[4 + length..8 + length].try_into().unwrap()) as usize,
            body.len() - 8 - length
        );
    }
}
