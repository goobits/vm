//! Deterministic release jobs kept separate from their CLI entry points.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::runtime::command_text;

pub mod package;
pub mod tool;

fn git() -> Command {
    let mut command = Command::new("git");
    command.env("GIT_TERMINAL_PROMPT", "0");
    if let Ok(token_file) = std::env::var("PKG_RELEASE_GIT_TOKEN_FILE") {
        command
            .env("GIT_ASKPASS", "pkg-git-askpass")
            .env("PKG_WORK_GIT_TOKEN_FILE", token_file);
    }
    command
}

fn git_text(repository: &Path, arguments: &[&str], operation: &str) -> Result<String> {
    Ok(
        command_text(git().arg("-C").arg(repository).args(arguments), operation)?
            .trim()
            .to_string(),
    )
}

fn file_digest(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("open immutable source bundle {}", path.display()))?;
    Ok(vm_packages::sha256_reader(std::io::BufReader::new(file))?.0)
}
