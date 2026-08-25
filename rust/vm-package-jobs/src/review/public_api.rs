use std::path::Path;

use anyhow::{Context, Result};
use vm_packages::{PackageEcosystem, SourceKind};

use super::checkout::file_at;

pub(super) fn public_api_paths(
    source_kind: SourceKind,
    ecosystem: Option<PackageEcosystem>,
    paths: &[String],
    manifest_is_public: bool,
) -> Vec<String> {
    paths
        .iter()
        .filter(|path| match (source_kind, ecosystem) {
            (SourceKind::Package, Some(PackageEcosystem::Cargo)) => {
                (path.as_str() == "Cargo.toml" && manifest_is_public) || path.starts_with("src/")
            }
            (SourceKind::Package, Some(PackageEcosystem::Npm)) => {
                (path.as_str() == "package.json" && manifest_is_public)
                    || path.starts_with("src/")
                    || path.ends_with(".d.ts")
            }
            (SourceKind::Package, Some(PackageEcosystem::Python)) => {
                path.ends_with(".py")
                    && !path.starts_with("tests/")
                    && !path.contains("/__pycache__/")
            }
            (SourceKind::ToolCollection, None) => {
                (path.as_str() == "package.json" && manifest_is_public)
                    || path.as_str() == "SKILL.md"
                    || path.ends_with("/SKILL.md")
            }
            (SourceKind::ToolBinary, None) => path.as_str() == "vm-tool.yaml",
            _ => false,
        })
        .cloned()
        .collect()
}

pub(super) fn manifest_has_public_changes(
    source_kind: SourceKind,
    ecosystem: Option<PackageEcosystem>,
    repository: &Path,
    initial_release: bool,
    base_commit: &str,
    submitted_commit: &str,
    paths: &[String],
) -> Result<bool> {
    let manifest = match (source_kind, ecosystem) {
        (SourceKind::Package, Some(PackageEcosystem::Cargo)) => "Cargo.toml",
        (SourceKind::Package, Some(PackageEcosystem::Npm)) | (SourceKind::ToolCollection, None) => {
            "package.json"
        }
        (SourceKind::ToolBinary, None) => "vm-tool.yaml",
        _ => return Ok(false),
    };
    if !paths.iter().any(|path| path == manifest) {
        return Ok(false);
    }
    if initial_release {
        return Ok(true);
    }
    let Some(base) = file_at(repository, base_commit, manifest)? else {
        return Ok(true);
    };
    let Some(submitted) = file_at(repository, submitted_commit, manifest)? else {
        return Ok(true);
    };
    manifest_content_has_public_changes(source_kind, ecosystem, &base, &submitted)
}

fn manifest_content_has_public_changes(
    source_kind: SourceKind,
    ecosystem: Option<PackageEcosystem>,
    base: &str,
    submitted: &str,
) -> Result<bool> {
    match (source_kind, ecosystem) {
        (SourceKind::Package, Some(PackageEcosystem::Cargo)) => {
            let mut base: toml::Value =
                toml::from_str(base).context("base Cargo.toml is invalid")?;
            let mut submitted: toml::Value =
                toml::from_str(submitted).context("submitted Cargo.toml is invalid")?;
            remove_cargo_versions(&mut base);
            remove_cargo_versions(&mut submitted);
            Ok(base != submitted)
        }
        (SourceKind::Package, Some(PackageEcosystem::Npm)) | (SourceKind::ToolCollection, None) => {
            let mut base: serde_json::Value =
                serde_json::from_str(base).context("base package.json is invalid")?;
            let mut submitted: serde_json::Value =
                serde_json::from_str(submitted).context("submitted package.json is invalid")?;
            remove_json_version(&mut base);
            remove_json_version(&mut submitted);
            Ok(base != submitted)
        }
        (SourceKind::ToolBinary, None) => {
            let mut base: serde_yaml_ng::Value =
                serde_yaml_ng::from_str(base).context("base vm-tool.yaml is invalid")?;
            let mut submitted: serde_yaml_ng::Value =
                serde_yaml_ng::from_str(submitted).context("submitted vm-tool.yaml is invalid")?;
            remove_yaml_version(&mut base);
            remove_yaml_version(&mut submitted);
            Ok(base != submitted)
        }
        _ => Ok(false),
    }
}

fn remove_json_version(manifest: &mut serde_json::Value) {
    if let Some(table) = manifest.as_object_mut() {
        table.remove("version");
    }
}

fn remove_cargo_versions(manifest: &mut toml::Value) {
    if let Some(package) = manifest
        .get_mut("package")
        .and_then(toml::Value::as_table_mut)
    {
        package.remove("version");
    }
    if let Some(package) = manifest
        .get_mut("workspace")
        .and_then(toml::Value::as_table_mut)
        .and_then(|workspace| workspace.get_mut("package"))
        .and_then(toml::Value::as_table_mut)
    {
        package.remove("version");
    }
}

fn remove_yaml_version(manifest: &mut serde_yaml_ng::Value) {
    if let Some(mapping) = manifest.as_mapping_mut() {
        mapping.remove(serde_yaml_ng::Value::String("version".into()));
    }
}

pub(super) fn removed_public_surface(diff: &str) -> bool {
    diff.lines().any(|line| {
        let removed = line.strip_prefix('-').unwrap_or_default().trim_start();
        removed.starts_with("pub ")
            || removed.starts_with("pub(")
            || removed.starts_with("export ")
            || removed.starts_with("def ")
            || removed.starts_with("class ")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_public_api_paths() {
        let paths = vec!["src/lib.rs".into(), "target/debug/output".into()];
        assert_eq!(
            public_api_paths(
                SourceKind::Package,
                Some(PackageEcosystem::Cargo),
                &paths,
                true
            ),
            ["src/lib.rs"]
        );
        assert!(removed_public_surface("-pub fn removed() {}"));
        assert!(public_api_paths(
            SourceKind::ToolCollection,
            None,
            &["package.json".into(), "README.md".into()],
            false
        )
        .is_empty());
        assert_eq!(
            public_api_paths(
                SourceKind::ToolCollection,
                None,
                &["package.json".into()],
                true
            ),
            ["package.json"]
        );
    }

    #[test]
    fn manifest_classification_ignores_only_release_versions() {
        assert!(!manifest_content_has_public_changes(
            SourceKind::ToolCollection,
            None,
            r#"{"name":"agent-skills","version":"1.0.0"}"#,
            r#"{"name":"agent-skills","version":"1.0.1"}"#,
        )
        .unwrap());
        assert!(manifest_content_has_public_changes(
            SourceKind::Package,
            Some(PackageEcosystem::Npm),
            r#"{"name":"auth","version":"1.0.0","exports":"./index.js"}"#,
            r#"{"name":"auth","version":"1.0.1","exports":"./src/index.js"}"#,
        )
        .unwrap());
        assert!(!manifest_content_has_public_changes(
            SourceKind::Package,
            Some(PackageEcosystem::Cargo),
            "[package]\nname='auth'\nversion='1.0.0'\n",
            "[package]\nname='auth'\nversion='1.0.1'\n",
        )
        .unwrap());
        assert!(manifest_content_has_public_changes(
            SourceKind::Package,
            Some(PackageEcosystem::Cargo),
            "[package]\nname='auth'\nversion='1.0.0'\n",
            "[package]\nname='auth'\nversion='1.0.1'\n[dependencies]\nserde='1'\n",
        )
        .unwrap());
    }
}
