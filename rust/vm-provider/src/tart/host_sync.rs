use std::path::{Path, PathBuf};

use crate::user_home::resolve_home_dir as resolve_real_home_dir;
use vm_config::config::VmConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSyncMount {
    pub tag: String,
    pub host_path: PathBuf,
    pub guest_path: String,
}

pub fn resolve_home_dir() -> Option<PathBuf> {
    resolve_real_home_dir()
}

pub fn expand_tilde(path: &str) -> Option<PathBuf> {
    if path == "~" {
        return resolve_home_dir();
    }

    if let Some(suffix) = path.strip_prefix("~/") {
        return resolve_home_dir().map(|home| home.join(suffix));
    }

    Some(PathBuf::from(path))
}

pub fn collect_host_sync_mounts(config: &VmConfig) -> Vec<HostSyncMount> {
    let mut mounts = Vec::new();
    let Some(host_sync) = config.host_sync.as_ref() else {
        return mounts;
    };
    let Some(home) = resolve_home_dir() else {
        return mounts;
    };

    if let Some(ai_tools) = host_sync.ai_tools.as_ref() {
        add_mount_if_exists(
            &mut mounts,
            "claude-sync",
            home.join(".claude"),
            "~/.claude".to_string(),
            ai_tools.is_claude_enabled(),
        );
        add_mount_if_exists(
            &mut mounts,
            "gemini-sync",
            home.join(".gemini"),
            "~/.gemini".to_string(),
            ai_tools.is_gemini_enabled(),
        );
        add_mount_if_exists(
            &mut mounts,
            "codex-sync",
            home.join(".codex"),
            "~/.codex".to_string(),
            ai_tools.is_codex_enabled(),
        );
    }

    mounts
}

fn add_mount_if_exists(
    mounts: &mut Vec<HostSyncMount>,
    tag: &str,
    host_path: PathBuf,
    guest_path: String,
    enabled: bool,
) {
    if !enabled || !host_path.exists() || !host_path.is_dir() {
        return;
    }

    mounts.push(HostSyncMount {
        tag: tag.to_string(),
        host_path,
        guest_path,
    });
}

pub fn resolve_guest_home_path(path: &str) -> String {
    if path == "~" {
        "$HOME".to_string()
    } else if let Some(suffix) = path.strip_prefix("~/") {
        format!("$HOME/{suffix}")
    } else {
        path.to_string()
    }
}

pub fn file_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(std::string::ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::{file_name, resolve_guest_home_path};
    use std::path::Path;

    #[test]
    fn resolves_guest_home_path() {
        assert_eq!(resolve_guest_home_path("~/.claude"), "$HOME/.claude");
        assert_eq!(resolve_guest_home_path("/workspace"), "/workspace");
    }

    #[test]
    fn extracts_file_name() {
        assert_eq!(
            file_name(Path::new("/tmp/test.txt")).as_deref(),
            Some("test.txt")
        );
    }
}
