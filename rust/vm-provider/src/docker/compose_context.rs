use std::borrow::Cow;
use std::fs;
use std::path::Path;
use std::process::Command;

use tera::Context as TeraContext;
use vm_config::{config::VmConfig, detect_worktrees_in};
use vm_core::{
    error::{Result, VmError},
    vm_warning,
};

use crate::user_home::resolve_home_dir;
use crate::ProviderContext;

pub(super) fn ensure_ai_sync_dirs(config: &VmConfig) -> Result<()> {
    let Some(ai_sync) = config
        .host_sync
        .as_ref()
        .and_then(|host_sync| host_sync.ai_tools.as_ref())
    else {
        return Ok(());
    };
    let home = resolve_home_dir()
        .ok_or_else(|| VmError::Internal("HOME environment variable not set".to_string()))?;
    let project = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");

    for (enabled, tool) in [
        (ai_sync.is_claude_enabled(), "claude"),
        (ai_sync.is_antigravity_enabled(), "gemini"),
        (ai_sync.is_codex_enabled(), "codex"),
    ] {
        if !enabled {
            continue;
        }
        let directory = home.join(".vm").join("ai-sync").join(tool).join(project);
        fs::create_dir_all(&directory).map_err(|error| {
            VmError::Internal(format!(
                "Failed to create {tool} sync directory '{}': {error}",
                directory.display()
            ))
        })?;
        maybe_chown_path_to_sudo_user(&directory);
    }

    Ok(())
}

pub(super) fn build_service_environment(
    config: &VmConfig,
    context: &ProviderContext,
) -> Vec<(String, String)> {
    let mut environment = Vec::new();
    if let Some(global) = context.global_config.as_ref() {
        append_service_environment(config, global, &mut environment);
    }
    environment
}

fn append_service_environment(
    config: &VmConfig,
    global: &vm_config::GlobalConfig,
    environment: &mut Vec<(String, String)>,
) {
    let host = vm_platform::platform::get_host_gateway();
    if global.services.postgresql.enabled {
        let port = global.services.postgresql.port;
        let database = config
            .project
            .as_ref()
            .and_then(|project| project.name.as_deref())
            .unwrap_or("vm_project");
        environment.push((
            "DATABASE_URL".to_string(),
            format!("postgresql://postgres:postgres@{host}:{port}/{database}"),
        ));
    }
    if global.services.redis.enabled {
        environment.push((
            "REDIS_URL".to_string(),
            format!("redis://{host}:{}", global.services.redis.port),
        ));
    }
    if global.services.mongodb.enabled {
        environment.push((
            "MONGODB_URL".to_string(),
            format!("mongodb://{host}:{}", global.services.mongodb.port),
        ));
    }
}

pub(super) fn configure_ssh_agent(config: &VmConfig, context: &mut TeraContext) {
    let enabled = config
        .host_sync
        .as_ref()
        .is_some_and(|host_sync| host_sync.ssh_agent);
    if !enabled {
        return;
    }
    let Ok(socket) = std::env::var("SSH_AUTH_SOCK") else {
        return;
    };
    context.insert("ssh_auth_sock", &socket);

    let mount_config = config
        .host_sync
        .as_ref()
        .map(|host_sync| host_sync.ssh_config)
        .unwrap_or(enabled);
    if mount_config {
        let path = resolve_home_dir().map(|home| home.join(".ssh/config"));
        if let Some(path) = path.filter(|path| path.exists()) {
            context.insert("ssh_config_path", &path.to_string_lossy().to_string());
        }
    }
}

pub(super) fn process_dotfiles(config: &VmConfig, username: &str) -> Vec<(String, String)> {
    let Some(host_sync) = config.host_sync.as_ref() else {
        return Vec::new();
    };
    host_sync
        .dotfiles
        .iter()
        .filter_map(|configured_path| {
            let expanded = expand_tilde(configured_path)?;
            if !Path::new(expanded.as_ref()).exists() {
                vm_warning!("Dotfile not found, skipping: {expanded}");
                return None;
            }
            let target = if let Some(relative) = configured_path.strip_prefix("~/") {
                format!("/home/{username}/{relative}")
            } else if configured_path == "~" {
                format!("/home/{username}")
            } else if configured_path.starts_with('/') {
                configured_path.clone()
            } else {
                format!("/home/{username}/{configured_path}")
            };
            Some((expanded.into_owned(), target))
        })
        .collect()
}

pub(super) fn configure_worktrees(
    config: &VmConfig,
    context: &mut TeraContext,
    project_dir: &Path,
    workspace_path: &Path,
    home_dir: &str,
    project: &str,
    create_directory: bool,
) {
    if !worktrees_enabled(config) {
        return;
    }

    let base = Path::new(home_dir).join(".vm/worktrees").join(project);
    let managed_base = if !create_directory || fs::create_dir_all(&base).is_ok() {
        context.insert("worktrees_base_dir", &base.to_string_lossy().to_string());
        Some(base.as_path())
    } else {
        vm_warning!("Failed to create worktrees directory {}", base.display());
        None
    };

    let mounts = resolve_worktree_mounts(
        workspace_path,
        detect_worktrees_in(project_dir).unwrap_or_default(),
        managed_base,
    );
    if !mounts.is_empty() {
        context.insert("worktrees", &mounts);
    }
}

pub(super) fn worktrees_enabled(config: &VmConfig) -> bool {
    config
        .host_sync
        .as_ref()
        .and_then(|host_sync| host_sync.worktrees.as_ref())
        .map(|worktrees| worktrees.enabled)
        .unwrap_or_else(|| {
            vm_config::GlobalConfig::load()
                .ok()
                .map(|global| global.worktrees.enabled)
                .unwrap_or(true)
        })
}

pub(super) fn resolve_worktree_mounts(
    workspace_path: &Path,
    worktrees: Vec<String>,
    covered_source_root: Option<&Path>,
) -> Vec<(String, String)> {
    worktrees
        .into_iter()
        .filter_map(|source| {
            let source_path = Path::new(&source);
            if covered_source_root.is_some_and(|root| source_path.starts_with(root)) {
                return None;
            }
            let name = source_path.file_name()?;
            let target = workspace_path.join(name);
            Some((source, target.to_str()?.to_string()))
        })
        .collect()
}

fn expand_tilde(path: &str) -> Option<Cow<'_, str>> {
    match path {
        "~" => resolve_home_dir().map(|home| Cow::Owned(home.to_string_lossy().to_string())),
        path if path.starts_with("~/") => {
            let home = resolve_home_dir()?.to_string_lossy().to_string();
            Some(Cow::Owned(path.replacen('~', &home, 1)))
        }
        path => Some(Cow::Borrowed(path)),
    }
}

fn maybe_chown_path_to_sudo_user(path: &Path) {
    #[cfg(unix)]
    {
        let (Ok(uid), Ok(gid)) = (std::env::var("SUDO_UID"), std::env::var("SUDO_GID")) else {
            return;
        };
        let owner = format!("{uid}:{gid}");
        let _ = Command::new("chown")
            .args(["-R", &owner, path.to_string_lossy().as_ref()])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::{HostSyncConfig, WorktreesConfig};

    #[test]
    fn resolves_worktree_targets_below_the_workspace() {
        let mounts = resolve_worktree_mounts(
            Path::new("/source"),
            vec!["/tmp/worktrees/feature".to_string()],
            None,
        );

        assert_eq!(
            mounts,
            vec![(
                "/tmp/worktrees/feature".to_string(),
                "/source/feature".to_string()
            )]
        );
    }

    #[test]
    fn omits_worktrees_already_covered_by_the_managed_base_mount() {
        let mounts = resolve_worktree_mounts(
            Path::new("/workspace"),
            vec![
                "/Users/miko/.vm/worktrees/vm/storage-5x-port".to_string(),
                "/Users/miko/other-worktree".to_string(),
            ],
            Some(Path::new("/Users/miko/.vm/worktrees/vm")),
        );

        assert_eq!(
            mounts,
            vec![(
                "/Users/miko/other-worktree".to_string(),
                "/workspace/other-worktree".to_string()
            )]
        );
    }

    #[test]
    fn falls_back_to_individual_mounts_when_the_managed_base_cannot_be_created() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let project = directory.path().join("project");
        let worktree = directory.path().join("feature");
        let metadata = project.join(".git/worktrees/feature");
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join(".vm"), "not a directory").unwrap();
        fs::create_dir_all(&metadata).unwrap();
        fs::create_dir(&worktree).unwrap();
        fs::write(
            metadata.join("gitdir"),
            worktree.join(".git").to_string_lossy().as_bytes(),
        )
        .unwrap();
        let config = VmConfig {
            host_sync: Some(HostSyncConfig {
                worktrees: Some(WorktreesConfig::default()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut context = TeraContext::new();

        configure_worktrees(
            &config,
            &mut context,
            &project,
            Path::new("/workspace"),
            home.to_str().unwrap(),
            "demo",
            true,
        );

        assert!(context.get("worktrees_base_dir").is_none());
        let mounts: Vec<(String, String)> =
            serde_json::from_value(context.get("worktrees").unwrap().clone()).unwrap();
        assert_eq!(
            mounts,
            vec![(
                worktree
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                "/workspace/feature".into()
            )]
        );
    }
}
