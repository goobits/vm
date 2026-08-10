//! Git configuration detection.
//
//! This module provides functionality for detecting and parsing Git configuration
//! from the host system.

use std::fs;
use std::path::{Path, PathBuf};

use git2::{Config, Repository};
use serde::{Deserialize, Serialize};
use vm_core::error::{Result, VmError};

/// Represents the Git configuration extracted from the host.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitConfig {
    pub user_name: Option<String>,
    pub user_email: Option<String>,
    pub pull_rebase: Option<String>,
    pub init_default_branch: Option<String>,
    pub core_editor: Option<String>,
    pub core_excludesfile_content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryFacts {
    pub root: PathBuf,
    pub origin_url: String,
    pub default_branch: Option<String>,
}

/// Detects and parses the Git configuration from the host system.
pub fn detect_git_config() -> Result<GitConfig> {
    let mut config = GitConfig::default();

    if let Ok(git_config) = Config::open_default() {
        if let Ok(name) = git_config.get_string("user.name") {
            config.user_name = Some(name);
        }
        if let Ok(email) = git_config.get_string("user.email") {
            config.user_email = Some(email);
        }
        if let Ok(rebase) = git_config.get_string("pull.rebase") {
            config.pull_rebase = Some(rebase);
        }
        if let Ok(branch) = git_config.get_string("init.defaultBranch") {
            config.init_default_branch = Some(branch);
        }
        if let Ok(editor) = git_config.get_string("core.editor") {
            config.core_editor = Some(editor);
        }
        if let Ok(excludesfile) = git_config.get_path("core.excludesfile") {
            if let Ok(content) = fs::read_to_string(excludesfile) {
                config.core_excludesfile_content = Some(content);
            }
        }
    }

    Ok(config)
}

/// Detect the canonical root, origin, and default branch of one Git worktree.
pub fn detect_repository(path: &Path) -> Result<RepositoryFacts> {
    let repository = Repository::discover(path).map_err(|error| {
        VmError::validation(
            format!("{} is not inside a Git repository: {error}", path.display()),
            None::<String>,
        )
    })?;
    let root = repository.workdir().ok_or_else(|| {
        VmError::validation(
            format!("{} is a bare Git repository", path.display()),
            None::<String>,
        )
    })?;
    let root = fs::canonicalize(root)?;
    let origin = repository.find_remote("origin").map_err(|_| {
        VmError::validation(
            format!("Git repository {} has no origin remote", root.display()),
            None::<String>,
        )
    })?;
    let origin_url = origin
        .url()
        .ok()
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| {
            VmError::validation(
                format!("Git repository {} has an empty origin URL", root.display()),
                None::<String>,
            )
        })?;

    Ok(RepositoryFacts {
        root,
        origin_url: origin_url.to_string(),
        default_branch: default_branch(&repository),
    })
}

fn default_branch(repository: &Repository) -> Option<String> {
    if let Ok(reference) = repository.find_reference("refs/remotes/origin/HEAD") {
        if let Ok(Some(target)) = reference.symbolic_target() {
            if let Some(branch) = target.strip_prefix("refs/remotes/origin/") {
                return Some(branch.to_string());
            }
        }
    }

    repository
        .head()
        .ok()
        .and_then(|head| head.shorthand().ok().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::detect_repository;
    use git2::Repository;

    #[test]
    fn detects_worktree_root_origin_and_remote_default_branch() {
        let directory = tempfile::tempdir().unwrap();
        let repository = Repository::init(directory.path()).unwrap();
        repository
            .remote("origin", "https://example.com/shared/auth.git")
            .unwrap();
        repository
            .reference_symbolic(
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
                true,
                "test default branch",
            )
            .unwrap();
        let nested = directory.path().join("nested");
        std::fs::create_dir(&nested).unwrap();

        let facts = detect_repository(&nested).unwrap();

        assert_eq!(facts.root, directory.path().canonicalize().unwrap());
        assert_eq!(facts.origin_url, "https://example.com/shared/auth.git");
        assert_eq!(facts.default_branch.as_deref(), Some("main"));
    }
}
