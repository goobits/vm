//! Host mountpoint preparation for nested mounts below read-only workspaces.

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use vm_config::config::{MountAccess, VmConfig};
use vm_core::error::{Result, VmError};

use crate::Mount;

use super::compose_context::worktree_mount_plan;

/// Prepare directories that runc cannot create after attaching the read-only
/// workspace bind. Existing directories are left unchanged.
pub(super) fn prepare(
    config: &VmConfig,
    project_dir: &Path,
    extra_mounts: Option<&[Mount]>,
) -> Result<()> {
    let Some(project) = config.project.as_ref() else {
        return Ok(());
    };
    if project.workspace_access != MountAccess::ReadOnly {
        return Ok(());
    }

    let workspace = Path::new(project.workspace_path.as_deref().unwrap_or("/workspace"));
    let project_dir = project_dir.canonicalize().map_err(|error| {
        VmError::filesystem(error, project_dir.display(), "resolve project directory")
    })?;
    let worktree_mounts = worktree_mount_plan(config, &project_dir, workspace, true).mounts;
    let targets = std::iter::once(workspace.join("node_modules"))
        .chain(
            config
                .storage
                .volumes
                .values()
                .map(|volume| PathBuf::from(&volume.target)),
        )
        .chain(
            config
                .storage
                .tmpfs
                .iter()
                .map(|mount| PathBuf::from(&mount.target)),
        )
        .chain(config.mounts.iter().map(|mount| mount.target.clone()))
        .chain(
            worktree_mounts
                .into_iter()
                .map(|(_, target)| PathBuf::from(target)),
        )
        .chain(
            extra_mounts
                .unwrap_or_default()
                .iter()
                .map(|mount| mount.target.clone()),
        )
        .collect::<BTreeSet<_>>();

    for target in targets {
        let Ok(relative) = target.strip_prefix(workspace) else {
            continue;
        };
        if !relative.as_os_str().is_empty() {
            ensure_directory(&project_dir, relative, &target)?;
        }
    }
    Ok(())
}

fn ensure_directory(root: &Path, relative: &Path, guest_target: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(VmError::Config(format!(
                "Docker mount target '{}' must stay below the read-only workspace",
                guest_target.display()
            )));
        };
        current.push(name);

        loop {
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(VmError::Filesystem(format!(
                        "refusing symlink mountpoint '{}' for Docker target '{}'",
                        current.display(),
                        guest_target.display()
                    )));
                }
                Ok(metadata) if metadata.is_dir() => break,
                Ok(_) => {
                    return Err(VmError::Filesystem(format!(
                        "Docker mountpoint '{}' for target '{}' is not a directory",
                        current.display(),
                        guest_target.display()
                    )));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    match fs::create_dir(&current) {
                        Ok(()) => {
                            tracing::debug!(
                                "Prepared host mountpoint '{}' for read-only workspace target '{}'",
                                current.display(),
                                guest_target.display()
                            );
                            break;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                        Err(error) => {
                            return Err(VmError::filesystem(
                                error,
                                current.display(),
                                "create Docker mountpoint",
                            ));
                        }
                    }
                }
                Err(error) => {
                    return Err(VmError::filesystem(
                        error,
                        current.display(),
                        "inspect Docker mountpoint",
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::{
        MemoryLimit, ProjectConfig, TmpfsMountConfig, VolumeMountConfig, VolumeRetention,
        VolumeScope,
    };

    fn config(access: MountAccess) -> VmConfig {
        VmConfig {
            project: Some(ProjectConfig {
                name: Some("project".to_string()),
                workspace_path: Some("/workspace".to_string()),
                workspace_access: access,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn prepares_nested_mountpoints_idempotently() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        fs::create_dir(&project).unwrap();
        let mut config = config(MountAccess::ReadOnly);
        config.storage.volumes.insert(
            "build_cache".to_string(),
            VolumeMountConfig {
                target: "/workspace/.cache/build".to_string(),
                scope: VolumeScope::Project,
                nocopy: true,
                retention: VolumeRetention::Keep,
            },
        );
        config.storage.tmpfs.push(TmpfsMountConfig {
            target: "/workspace/tmp".to_string(),
            size: MemoryLimit::Limited(64),
            mode: "1777".to_string(),
        });
        let extra = Mount::with_target(
            root.path().join("source"),
            PathBuf::from("/workspace/packages/shared"),
            MountAccess::ReadOnly,
        );

        std::thread::scope(|scope| {
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
            for _ in 0..8 {
                let barrier = barrier.clone();
                let config = &config;
                let project = &project;
                let extra = &extra;
                scope.spawn(move || {
                    barrier.wait();
                    prepare(config, project, Some(std::slice::from_ref(extra))).unwrap();
                });
            }
        });
        prepare(&config, &project, Some(std::slice::from_ref(&extra))).unwrap();

        for path in ["node_modules", ".cache/build", "tmp", "packages/shared"] {
            assert!(project.join(path).is_dir(), "missing {path}");
        }
    }

    #[test]
    fn leaves_read_write_workspaces_unchanged() {
        let root = tempfile::tempdir().unwrap();
        prepare(&config(MountAccess::ReadWrite), root.path(), None).unwrap();
        assert!(!root.path().join("node_modules").exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_mountpoints() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, root.path().join("node_modules")).unwrap();

        let error = prepare(&config(MountAccess::ReadOnly), root.path(), None).unwrap_err();
        assert!(error.to_string().contains("refusing symlink mountpoint"));
        assert!(outside.read_dir().unwrap().next().is_none());
    }
}
