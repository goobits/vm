//! Git configuration detection.
//
//! This module provides functionality for detecting and parsing Git configuration
//! from the host system.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use vm_core::error::Result;

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

use git2::Config;
use std::fs;

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
