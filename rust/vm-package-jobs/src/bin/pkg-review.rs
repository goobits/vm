use std::path::{Component, Path};
use std::process::Command;

use anyhow::{bail, Context, Result};
use vm_package_jobs::runtime::{command_text, download_bundle, operation_key, run_command};
use vm_packages::{
    PackageEcosystem, PackageInfrastructureClient, PublicApiDiff, RegistryEndpoints,
    ReviewDecision, ReviewRequest, SourceKind, VersionRecommendation, WorkflowState,
};

#[tokio::main]
async fn main() -> Result<()> {
    let gateway =
        std::env::var("PKG_REVIEW_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let token = std::env::var("PKG_REVIEW_TOKEN").context("PKG_REVIEW_TOKEN is required")?;
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(gateway)?)
        .with_reviewer_token(&token);
    loop {
        match client.next_review().await {
            Ok(Some(submission)) => {
                if let Err(error) = review(&client, &token, &submission.submission_id).await {
                    eprintln!("review {} failed: {error:#}", submission.submission_id);
                }
            }
            Ok(None) => {}
            Err(error) => eprintln!("review queue unavailable: {error:#}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn review(
    client: &PackageInfrastructureClient,
    token: &str,
    submission_id: &str,
) -> Result<()> {
    let submission = client.submission(submission_id).await?;
    if submission.state != WorkflowState::Reviewing
        || !submission
            .validation
            .as_ref()
            .is_some_and(|result| result.passed())
    {
        bail!("submission is not ready for review");
    }
    let checkout = client.checkout(&submission.checkout_id).await?;
    let ecosystem = match checkout.source_kind {
        SourceKind::Package => Some(
            client
                .package_definition(&submission.package)
                .await?
                .ecosystem,
        ),
        SourceKind::ToolBinary | SourceKind::ToolCollection => None,
    };
    let review_root = tempfile::tempdir()?;
    let bundle = review_root.path().join("submission.bundle");
    download_bundle(
        &client.review_bundle_url(&submission.submission_id),
        token,
        &bundle,
    )?;
    let source = review_root.path().join("source");
    run_command(
        Command::new("git").arg("clone").arg(&bundle).arg(&source),
        "clone package review source",
    )?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .arg("checkout")
            .arg("--detach")
            .arg(&submission.submitted_commit),
        "check out package review commit",
    )?;

    let diff_base = if checkout.initial_release {
        command_text(
            Command::new("git")
                .arg("-C")
                .arg(&source)
                .stdin(std::process::Stdio::null())
                .args(["hash-object", "-t", "tree", "-w", "--stdin"]),
            "create empty initial-release tree",
        )?
        .trim()
        .to_string()
    } else {
        submission.base_commit.clone()
    };
    let range = format!("{diff_base}..{}", submission.submitted_commit);
    let changed_paths = git_lines(
        &source,
        &["diff", "--name-only", &range],
        "list changed package paths",
    )?;
    let diff = command_text(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["diff", "--unified=0", &range]),
        "inspect package review diff",
    )?;
    let manifest_is_public = manifest_has_public_changes(
        checkout.source_kind,
        ecosystem,
        &source,
        checkout.initial_release,
        &submission.base_commit,
        &submission.submitted_commit,
        &changed_paths,
    )?;
    let api_paths = public_api_paths(
        checkout.source_kind,
        ecosystem,
        &changed_paths,
        manifest_is_public,
    );
    let potentially_breaking = removed_public_surface(&diff);
    let api_diff = PublicApiDiff {
        changed_paths: api_paths.clone(),
        potentially_breaking,
    };

    let (decision, reason, required_followups) =
        if let Some(path) = sensitive_path(&source, &changed_paths) {
            (
                ReviewDecision::Reject,
                format!("sensitive file included: {path}"),
                vec!["Remove credentials or private files from the submission".into()],
            )
        } else if let Some(path) = generated_path(&changed_paths) {
            (
                ReviewDecision::NeedsChanges,
                format!("generated dependency/build output included: {path}"),
                vec!["Remove generated files from the submission".into()],
            )
        } else if !run_required_checks(checkout.source_kind, ecosystem, &source)? {
            (
                ReviewDecision::NeedsChanges,
                "required package checks failed in the isolated reviewer".into(),
                vec!["Fix package checks and resubmit".into()],
            )
        } else {
            (
                ReviewDecision::Approve,
                if api_paths.is_empty() {
                    "checks passed; no public API paths changed".into()
                } else {
                    format!(
                        "checks passed; {} public API path(s) changed",
                        api_paths.len()
                    )
                },
                Vec::new(),
            )
        };
    let recommended_version = if potentially_breaking {
        VersionRecommendation::Major
    } else if api_paths.is_empty() {
        VersionRecommendation::Patch
    } else {
        VersionRecommendation::Minor
    };
    let result = client
        .record_review(
            &submission.submission_id,
            &ReviewRequest {
                decision,
                recommended_version,
                api_diff,
                reason,
                required_followups,
                merge_strategy: "rebase".into(),
                reviewer: "ephemeral-integration-agent".into(),
                idempotency_key: operation_key(
                    "review",
                    &format!(
                        "{}:{}",
                        submission.submission_id, submission.submitted_commit
                    ),
                ),
            },
        )
        .await?;
    println!("{}: {:?}", result.submission_id, decision);
    Ok(())
}

fn run_required_checks(
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
            let manifest: vm_packages::ToolSourceManifest =
                serde_yaml_ng::from_slice(&std::fs::read(source.join("vm-tool.yaml"))?)?;
            manifest.validate()?;
            return Ok(manifest.kind == vm_packages::ToolKind::Binary);
        }
        (SourceKind::ToolCollection, None) => &[("npm", &["test", "--if-present"])],
        _ => bail!("source kind and package ecosystem do not match"),
    };
    for (program, arguments) in commands {
        let status = Command::new(program)
            .args(*arguments)
            .current_dir(source)
            .status()
            .with_context(|| format!("failed to launch required check {program}"))?;
        if !status.success() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn public_api_paths(
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

fn manifest_has_public_changes(
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
    let Some(base) = git_file(repository, base_commit, manifest)? else {
        return Ok(true);
    };
    let Some(submitted) = git_file(repository, submitted_commit, manifest)? else {
        return Ok(true);
    };
    manifest_content_has_public_changes(source_kind, ecosystem, &base, &submitted)
}

fn git_file(repository: &Path, commit: &str, path: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["show", &format!("{commit}:{path}")])
        .output()
        .with_context(|| format!("failed to inspect {path} at {commit}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8(output.stdout)?))
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
            if let Some(mapping) = base.as_mapping_mut() {
                mapping.remove(serde_yaml_ng::Value::String("version".into()));
            }
            if let Some(mapping) = submitted.as_mapping_mut() {
                mapping.remove(serde_yaml_ng::Value::String("version".into()));
            }
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

fn removed_public_surface(diff: &str) -> bool {
    diff.lines().any(|line| {
        let removed = line.strip_prefix('-').unwrap_or_default().trim_start();
        removed.starts_with("pub ")
            || removed.starts_with("pub(")
            || removed.starts_with("export ")
            || removed.starts_with("def ")
            || removed.starts_with("class ")
    })
}

fn sensitive_path<'a>(repository: &Path, paths: &'a [String]) -> Option<&'a str> {
    paths.iter().map(String::as_str).find(|path| {
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        name == ".env"
            || (name.starts_with(".env.")
                && !(name == ".env.example" && comment_only_environment_example(repository, path)))
            || name == "id_rsa"
            || name.contains("credential")
            || name.ends_with(".pem")
            || name.ends_with(".key")
    })
}

fn comment_only_environment_example(repository: &Path, path: &str) -> bool {
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

fn generated_path(paths: &[String]) -> Option<&str> {
    paths.iter().map(String::as_str).find(|path| {
        path.starts_with("node_modules/")
            || path.starts_with("target/")
            || path.starts_with(".venv/")
            || path.contains("/__pycache__/")
    })
}

fn git_lines(repository: &Path, arguments: &[&str], operation: &str) -> Result<Vec<String>> {
    Ok(command_text(
        Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(arguments),
        operation,
    )?
    .lines()
    .map(str::to_string)
    .filter(|line| !line.is_empty())
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_classification_detects_api_security_and_generated_changes() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir(repository.path().join("config")).unwrap();
        std::fs::write(
            repository.path().join("config/.env.example"),
            "# TOKEN=replace-me\n",
        )
        .unwrap();
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
        assert_eq!(generated_path(&paths), Some("target/debug/output"));
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
