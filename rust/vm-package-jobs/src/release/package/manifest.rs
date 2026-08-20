use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use semver::Version;
use serde::Deserialize;
use vm_packages::{PackageEcosystem, PackageIdentity};

use crate::runtime::run_command as run;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PackageManifest {
    pub(super) name: String,
    pub(super) version: Version,
}

pub(super) fn package_manifest(
    ecosystem: PackageEcosystem,
    source: &Path,
    expected_name: &str,
) -> Result<PackageManifest> {
    let identity = match ecosystem {
        PackageEcosystem::Npm => npm_manifest(source)?,
        PackageEcosystem::Cargo => cargo_manifest(source, expected_name)?,
        PackageEcosystem::Python => python_manifest(source)?,
    };
    let expected = PackageIdentity::new(ecosystem, expected_name)?;
    if !expected.matches_name(&identity.name) {
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

fn npm_manifest(source: &Path) -> Result<PackageManifest> {
    #[derive(Deserialize)]
    struct Manifest {
        name: String,
        version: String,
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(source.join("package.json"))?)?;
    Ok(PackageManifest {
        name: manifest.name,
        version: Version::parse(&manifest.version)?,
    })
}

fn cargo_manifest(source: &Path, expected_name: &str) -> Result<PackageManifest> {
    #[derive(Deserialize)]
    struct Metadata {
        packages: Vec<CargoPackage>,
    }
    #[derive(Deserialize)]
    struct CargoPackage {
        name: String,
        version: String,
    }
    let output = run(
        Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .arg("--manifest-path")
            .arg(source.join("Cargo.toml")),
        "read Cargo package metadata",
    )?;
    let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
    let expected = PackageIdentity::new(PackageEcosystem::Cargo, expected_name)?;
    let package = metadata
        .packages
        .into_iter()
        .find(|package| expected.matches_name(&package.name))
        .with_context(|| format!("Cargo workspace has no package named {expected_name}"))?;
    Ok(PackageManifest {
        name: package.name,
        version: Version::parse(&package.version)?,
    })
}

fn python_manifest(source: &Path) -> Result<PackageManifest> {
    const SCRIPT: &str = r#"import json, pathlib, sys, tomllib
data = tomllib.loads(pathlib.Path(sys.argv[1]).read_text())
project = data.get("project", {})
poetry = data.get("tool", {}).get("poetry", {})
name = project.get("name") or poetry.get("name")
version = project.get("version") or poetry.get("version")
if not name or not version:
    raise SystemExit("pyproject.toml must declare a static name and version")
print(json.dumps({"name": name, "version": version}))"#;
    let output = run(
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
    Ok(PackageManifest {
        name: identity.name,
        version: Version::parse(&identity.version)?,
    })
}
