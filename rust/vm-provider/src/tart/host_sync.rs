use std::fs;
use std::path::{Path, PathBuf};

use vm_config::config::VmConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSyncMount {
    pub tag: String,
    pub host_path: PathBuf,
    pub guest_path: String,
}

pub fn resolve_home_dir() -> Option<PathBuf> {
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        if !sudo_user.is_empty() && sudo_user != "root" {
            if let Some(home) = home_dir_from_passwd(&sudo_user) {
                return Some(home);
            }
        }
    }

    std::env::var("HOME").ok().map(PathBuf::from)
}

fn home_dir_from_passwd(user: &str) -> Option<PathBuf> {
    let contents = fs::read_to_string("/etc/passwd").ok()?;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.split(':');
        let name = parts.next()?;
        if name != user {
            continue;
        }

        let _passwd = parts.next()?;
        let _uid = parts.next()?;
        let _gid = parts.next()?;
        let _gecos = parts.next()?;
        let home = parts.next()?;

        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }

    None
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
    let project_name = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");

    if let Some(ai_tools) = host_sync.ai_tools.as_ref() {
        add_ai_sync_mount(
            &mut mounts,
            "claude-sync",
            &home,
            project_name,
            "claude",
            "~/.claude".to_string(),
            ai_tools.is_claude_enabled(),
        );
        add_ai_sync_mount(
            &mut mounts,
            "gemini-sync",
            &home,
            project_name,
            "gemini",
            "~/.gemini".to_string(),
            ai_tools.is_gemini_enabled(),
        );
        add_ai_sync_mount(
            &mut mounts,
            "codex-sync",
            &home,
            project_name,
            "codex",
            "~/.codex".to_string(),
            ai_tools.is_codex_enabled(),
        );
    }

    mounts
}

fn add_ai_sync_mount(
    mounts: &mut Vec<HostSyncMount>,
    tag: &str,
    home: &Path,
    project_name: &str,
    tool_name: &str,
    guest_path: String,
    enabled: bool,
) {
    if !enabled {
        return;
    }

    let host_path = home
        .join(".vm")
        .join("ai-sync")
        .join(tool_name)
        .join(project_name);
    if fs::create_dir_all(&host_path).is_err() || !host_path.is_dir() {
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
    use super::{collect_host_sync_mounts, file_name, resolve_guest_home_path};
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use vm_config::config::{
        AiSyncConfig, AiToolSyncConfig, HostSyncConfig, ProjectConfig, VmConfig,
    };

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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

    #[test]
    fn ai_tool_sync_uses_project_isolated_vm_dirs() {
        let _guard = env_lock().lock().unwrap();
        let temp_home = tempfile::tempdir().unwrap();
        let previous_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", temp_home.path());

        let config = VmConfig {
            project: Some(ProjectConfig {
                name: Some("demo".to_string()),
                ..Default::default()
            }),
            host_sync: Some(HostSyncConfig {
                ai_tools: Some(AiSyncConfig::Detailed(AiToolSyncConfig {
                    codex: true,
                    ..Default::default()
                })),
                ..Default::default()
            }),
            ..Default::default()
        };

        let mounts = collect_host_sync_mounts(&config);

        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }

        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].tag, "codex-sync");
        assert_eq!(mounts[0].guest_path, "~/.codex");
        assert!(mounts[0].host_path.ends_with(".vm/ai-sync/codex/demo"));
        assert!(mounts[0].host_path.is_dir());
    }
}
