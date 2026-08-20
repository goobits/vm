//! Deterministic release jobs kept separate from their CLI entry points.

use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::runtime::command_text;

pub mod package;
mod source;
pub mod tool;
mod workflow;

fn git() -> Command {
    let mut command = Command::new("git");
    command.env("GIT_TERMINAL_PROMPT", "0");
    if let Ok(token_file) = std::env::var("PKG_RELEASE_GIT_TOKEN_FILE") {
        for config in vm_packages::AUTHENTICATED_GIT_CONFIG {
            command.args(["-c", config]);
        }
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
