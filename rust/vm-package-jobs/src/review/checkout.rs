use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use vm_packages::{CheckoutRecord, PackageInfrastructureClient, SubmissionRecord};

use crate::runtime::{command_text, download_bundle, run_command};

pub(super) struct ReviewSource {
    _root: tempfile::TempDir,
    path: PathBuf,
    changed_paths: Vec<String>,
    diff: String,
}

impl ReviewSource {
    pub(super) fn prepare(
        client: &PackageInfrastructureClient,
        token: &str,
        submission: &SubmissionRecord,
        checkout: &CheckoutRecord,
    ) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let bundle = root.path().join("submission.bundle");
        download_bundle(
            &client.review_bundle_url(&submission.submission_id),
            token,
            &bundle,
        )?;
        let path = root.path().join("source");
        run_command(
            Command::new("git").arg("clone").arg(&bundle).arg(&path),
            "clone package review source",
        )?;
        run_command(
            Command::new("git")
                .arg("-C")
                .arg(&path)
                .arg("checkout")
                .arg("--detach")
                .arg(&submission.submitted_commit),
            "check out package review commit",
        )?;

        let diff_base = if checkout.initial_release {
            command_text(
                Command::new("git")
                    .arg("-C")
                    .arg(&path)
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
            &path,
            &["diff", "--name-only", &range],
            "list changed package paths",
        )?;
        let diff = command_text(
            Command::new("git")
                .arg("-C")
                .arg(&path)
                .args(["diff", "--unified=0", &range]),
            "inspect package review diff",
        )?;
        Ok(Self {
            _root: root,
            path,
            changed_paths,
            diff,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn changed_paths(&self) -> &[String] {
        &self.changed_paths
    }

    pub(super) fn diff(&self) -> &str {
        &self.diff
    }
}

pub(super) fn file_at(repository: &Path, commit: &str, path: &str) -> Result<Option<String>> {
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
