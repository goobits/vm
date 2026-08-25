use std::path::{Component, Path};
use std::process::Command;

use anyhow::{bail, Result};
use vm_core::command_capture::sanitized_diagnostic;
use vm_packages::{PackageEcosystem, SourceKind, ToolKind, ToolSourceManifest};

use crate::runtime::command_output;

pub(super) fn run_required_checks(
    source_kind: SourceKind,
    ecosystem: Option<PackageEcosystem>,
    source: &Path,
) -> Result<bool> {
    let commands: &[(&str, &[&str])] = match (source_kind, ecosystem) {
        (SourceKind::Package, Some(PackageEcosystem::Cargo)) => &[("cargo", &["test"])],
        (SourceKind::Package, Some(PackageEcosystem::Npm)) => &[
            ("npm", &["install", "--ignore-scripts"]),
            ("npm", &["test", "--if-present"]),
        ],
        (SourceKind::Package, Some(PackageEcosystem::Python)) => &[
            ("python", &["-m", "venv", "/tmp/package-review-venv"]),
            (
                "/tmp/package-review-venv/bin/pip",
                &["install", "--editable", ".[dev]"],
            ),
            ("/tmp/package-review-venv/bin/python", &["-m", "pytest"]),
        ],
        (SourceKind::ToolBinary, None) => {
            let manifest: ToolSourceManifest =
                serde_yaml_ng::from_slice(&std::fs::read(source.join("vm-tool.yaml"))?)?;
            manifest.validate()?;
            return Ok(manifest.kind == ToolKind::Binary);
        }
        (SourceKind::ToolCollection, None) => &[("npm", &["test", "--if-present"])],
        _ => bail!("source kind and package ecosystem do not match"),
    };
    for (program, arguments) in commands {
        let output = command_output(
            Command::new(program).args(*arguments).current_dir(source),
            "run required package review check",
        )?;
        if !output.status.success() {
            tracing::warn!(
                operation = "review_check",
                program,
                error = %sanitized_diagnostic(&output.stderr),
                "required package review check failed"
            );
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn sensitive_path<'a>(repository: &Path, paths: &'a [String]) -> Option<&'a str> {
    paths.iter().map(String::as_str).find(|path| {
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        name == ".env"
            || (name.starts_with(".env.")
                && !(name == ".env.example" && comment_only_example(repository, path)))
            || name == "id_rsa"
            || name.contains("credential")
            || name.ends_with(".pem")
            || name.ends_with(".key")
    })
}

fn comment_only_example(repository: &Path, path: &str) -> bool {
    let path = Path::new(path);
    if path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return false;
    }
    let path = repository.join(path);
    if std::fs::metadata(&path).map_or(true, |metadata| metadata.len() > 64 * 1024) {
        return false;
    }
    std::fs::read_to_string(path).is_ok_and(|content| {
        content
            .lines()
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
    })
}

pub(super) fn generated_path(paths: &[String]) -> Option<&str> {
    paths.iter().map(String::as_str).find(|path| {
        path.starts_with("node_modules/")
            || path.starts_with("target/")
            || path.starts_with(".venv/")
            || path.contains("/__pycache__/")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sensitive_and_generated_paths() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir(repository.path().join("config")).unwrap();
        std::fs::write(
            repository.path().join("config/.env.example"),
            "# TOKEN=replace-me\n",
        )
        .unwrap();
        assert_eq!(
            generated_path(&["src/lib.rs".into(), "target/debug/output".into()]),
            Some("target/debug/output")
        );
        assert_eq!(
            sensitive_path(repository.path(), &["config/.env".into()]),
            Some("config/.env")
        );
        assert_eq!(
            sensitive_path(repository.path(), &["config/.env.example".into()]),
            None
        );
        std::fs::write(
            repository.path().join("config/.env.example"),
            "TOKEN=replace-me\n",
        )
        .unwrap();
        assert_eq!(
            sensitive_path(repository.path(), &["config/.env.example".into()]),
            Some("config/.env.example")
        );
    }
}
