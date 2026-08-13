use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{bail, Context, Result};
use clap::Parser;
use vm_packages::{
    PackageEcosystem, PackageInfrastructureClient, PublicApiDiff, RegistryEndpoints,
    ReviewDecision, ReviewRequest, VersionRecommendation, WorkflowState,
};

#[derive(Parser)]
#[command(
    name = "pkg-review",
    version,
    about = "Ephemeral package integration reviewer"
)]
struct Cli {
    #[arg(long, required_unless_present = "watch")]
    submission: Option<String>,
    #[arg(long)]
    watch: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let gateway =
        std::env::var("PKG_REVIEW_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let token = std::env::var("PKG_REVIEW_TOKEN").context("PKG_REVIEW_TOKEN is required")?;
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(gateway)?)
        .with_reviewer_token(token);
    if cli.watch {
        loop {
            match client.next_review().await {
                Ok(Some(submission)) => {
                    if let Err(error) = review(&client, &submission.submission_id).await {
                        eprintln!("review {} failed: {error:#}", submission.submission_id);
                    }
                }
                Ok(None) => {}
                Err(error) => eprintln!("review queue unavailable: {error:#}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
    review(
        &client,
        cli.submission
            .as_deref()
            .context("--submission is required")?,
    )
    .await
}

async fn review(client: &PackageInfrastructureClient, submission_id: &str) -> Result<()> {
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
    let definition = client.package_definition(&submission.package).await?;
    let managed_source = PathBuf::from(
        checkout
            .worktree
            .as_deref()
            .context("checkout source is missing")?,
    );
    let expected = PathBuf::from("/data/agents")
        .join(&submission.checkout_id)
        .join("source");
    if managed_source != expected {
        bail!("checkout source is outside reviewer storage");
    }

    let review_root = tempfile::tempdir()?;
    let source = review_root.path().join("source");
    command(
        Command::new("git")
            .arg("clone")
            .arg(&managed_source)
            .arg(&source),
    )?;
    command(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .arg("checkout")
            .arg("--detach")
            .arg(&submission.submitted_commit),
    )?;

    let changed_paths = git_lines(
        &source,
        &[
            "diff",
            "--name-only",
            &format!(
                "{}..{}",
                submission.base_commit, submission.submitted_commit
            ),
        ],
    )?;
    let diff = git_text(
        &source,
        &[
            "diff",
            "--unified=0",
            &format!(
                "{}..{}",
                submission.base_commit, submission.submitted_commit
            ),
        ],
    )?;
    let api_paths = public_api_paths(definition.ecosystem, &changed_paths);
    let potentially_breaking = removed_public_surface(&diff);
    let api_diff = PublicApiDiff {
        changed_paths: api_paths.clone(),
        potentially_breaking,
    };

    let (decision, reason, required_followups) = if let Some(path) = sensitive_path(&changed_paths)
    {
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
    } else if !run_required_checks(definition.ecosystem, &source)? {
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
                idempotency_key: format!("review-{}", submission.submission_id),
            },
        )
        .await?;
    println!("{}: {:?}", result.submission_id, decision);
    Ok(())
}

fn run_required_checks(ecosystem: PackageEcosystem, source: &Path) -> Result<bool> {
    let commands: &[(&str, &[&str])] = match ecosystem {
        PackageEcosystem::Cargo => &[("cargo", &["test"])],
        PackageEcosystem::Npm => &[
            ("npm", &["install", "--ignore-scripts"]),
            ("npm", &["test", "--if-present"]),
        ],
        PackageEcosystem::Python => &[
            ("python", &["-m", "venv", "/tmp/package-review-venv"]),
            (
                "/tmp/package-review-venv/bin/pip",
                &["install", "--editable", ".[dev]"],
            ),
            ("/tmp/package-review-venv/bin/python", &["-m", "pytest"]),
        ],
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

fn public_api_paths(ecosystem: PackageEcosystem, paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| match ecosystem {
            PackageEcosystem::Cargo => path.as_str() == "Cargo.toml" || path.starts_with("src/"),
            PackageEcosystem::Npm => {
                path.as_str() == "package.json"
                    || path.starts_with("src/")
                    || path.ends_with(".d.ts")
            }
            PackageEcosystem::Python => {
                path.ends_with(".py")
                    && !path.starts_with("tests/")
                    && !path.contains("/__pycache__/")
            }
        })
        .cloned()
        .collect()
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

fn sensitive_path(paths: &[String]) -> Option<&str> {
    paths.iter().map(String::as_str).find(|path| {
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        name == ".env"
            || name.starts_with(".env.")
            || name == "id_rsa"
            || name.contains("credential")
            || name.ends_with(".pem")
            || name.ends_with(".key")
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

fn git_lines(repository: &Path, arguments: &[&str]) -> Result<Vec<String>> {
    Ok(git_text(repository, arguments)?
        .lines()
        .map(str::to_string)
        .filter(|line| !line.is_empty())
        .collect())
}

fn git_text(repository: &Path, arguments: &[&str]) -> Result<String> {
    let output = command(
        Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(arguments),
    )?;
    String::from_utf8(output.stdout).context("git returned invalid UTF-8")
}

fn command(command: &mut Command) -> Result<Output> {
    let output = command.output()?;
    if output.status.success() {
        Ok(output)
    } else {
        bail!("command failed with {}", output.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_classification_detects_api_security_and_generated_changes() {
        let paths = vec!["src/lib.rs".into(), "target/debug/output".into()];
        assert_eq!(
            public_api_paths(PackageEcosystem::Cargo, &paths),
            ["src/lib.rs"]
        );
        assert_eq!(generated_path(&paths), Some("target/debug/output"));
        assert_eq!(sensitive_path(&["config/.env".into()]), Some("config/.env"));
        assert!(removed_public_surface("-pub fn removed() {}"));
    }
}
