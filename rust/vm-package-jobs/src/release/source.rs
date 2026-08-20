use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::runtime::run_command as run;

use super::{git, git_text};

pub(super) fn file_digest(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("open immutable source bundle {}", path.display()))?;
    Ok(vm_packages::sha256_reader(std::io::BufReader::new(file))?.0)
}

pub(super) fn clone_at(bundle: &Path, destination: &Path, commit: &str) -> Result<()> {
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

pub(super) fn push_source(
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
    let local_tag = git_text(
        source,
        &["rev-parse", &format!("refs/tags/{tag}^{{}}")],
        "inspect local release tag",
    )
    .ok();
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
    let output = run(
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
    let output = run(
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
