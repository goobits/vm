use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::runtime::run_command;

const WORKSPACE_PREFIX: &str = "vm-build-";

pub(super) struct BuildWorkspace {
    directory: Option<tempfile::TempDir>,
}

impl BuildWorkspace {
    pub(super) fn create(root: &Path) -> Result<Self> {
        ensure_build_work_root(root)?;
        let directory = tempfile::Builder::new()
            .prefix(WORKSPACE_PREFIX)
            .tempdir_in(root)
            .with_context(|| format!("create binary build workspace in {}", root.display()))?;
        Ok(Self {
            directory: Some(directory),
        })
    }

    pub(super) fn path(&self) -> &Path {
        self.directory
            .as_ref()
            .expect("build workspace is available until cleanup")
            .path()
    }

    pub(super) fn close(mut self) -> Result<()> {
        let directory = self
            .directory
            .take()
            .expect("build workspace is cleaned exactly once");
        cleanup_tempdir(directory)
    }
}

impl Drop for BuildWorkspace {
    fn drop(&mut self) {
        let Some(directory) = self.directory.take() else {
            return;
        };
        let path = directory.path().to_path_buf();
        if let Err(error) = cleanup_tempdir(directory) {
            tracing::error!(
                operation = "cleanup_build_workspace",
                workspace = %path.display(),
                error = ?error,
                "binary build workspace cleanup failed"
            );
        }
    }
}

pub fn prepare_build_work_root(root: &Path) -> Result<()> {
    ensure_build_work_root(root)?;

    for entry in fs::read_dir(root)
        .with_context(|| format!("inspect binary build work root {}", root.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(WORKSPACE_PREFIX) {
            continue;
        }
        let file_type = entry.file_type()?;
        if !file_type.is_dir() || file_type.is_symlink() {
            bail!(
                "unexpected entry in binary build work root: {}",
                entry.path().display()
            );
        }
        cleanup_path(&entry.path())?;
        tracing::info!(
            operation = "cleanup_build_workspace",
            workspace = %entry.path().display(),
            outcome = "stale_removed",
            "removed stale binary build workspace"
        );
    }
    Ok(())
}

fn ensure_build_work_root(root: &Path) -> Result<()> {
    validate_work_root(root)?;
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("binary build work root cannot be a symbolic link")
        }
        Ok(metadata) if !metadata.is_dir() => bail!("binary build work root is not a directory"),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(root)
                .with_context(|| format!("create binary build work root {}", root.display()))?;
        }
        Err(error) => return Err(error.into()),
    }
    restrict_work_root(root)?;
    Ok(())
}

fn validate_work_root(root: &Path) -> Result<()> {
    if !root.is_absolute() || root.parent().is_none() || root.parent() == Some(Path::new("/")) {
        bail!("binary build work root must be a scoped absolute directory")
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_work_root(root: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(root)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(root, permissions)
        .with_context(|| format!("restrict binary build work root {}", root.display()))
}

#[cfg(not(unix))]
fn restrict_work_root(_root: &Path) -> Result<()> {
    Ok(())
}

fn cleanup_tempdir(directory: tempfile::TempDir) -> Result<()> {
    reclaim_workspace(directory.path())?;
    directory.close().context("remove binary build workspace")
}

fn cleanup_path(path: &Path) -> Result<()> {
    reclaim_workspace(path)?;
    fs::remove_dir_all(path)
        .with_context(|| format!("remove stale binary build workspace {}", path.display()))
}

fn reclaim_workspace(path: &Path) -> Result<()> {
    let Some(_) = std::env::var_os("PKG_BUILD_UID") else {
        return Ok(());
    };
    reclaim_unix_workspace(path)
}

#[cfg(unix)]
fn reclaim_unix_workspace(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let root = path
        .parent()
        .context("binary build workspace has no work root")?;
    let metadata = fs::metadata(root)?;
    let owner = format!("{}:{}", metadata.uid(), metadata.gid());
    run_command(
        Command::new("chown").args(["-R", &owner, "--"]).arg(path),
        "reclaim binary build workspace ownership",
    )?;
    run_command(
        Command::new("chmod").args(["-R", "u+rwX", "--"]).arg(path),
        "restore binary build workspace permissions",
    )?;
    Ok(())
}

#[cfg(not(unix))]
fn reclaim_unix_workspace(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_close_removes_only_its_directory() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("builder/work");
        let workspace = BuildWorkspace::create(&root).unwrap();
        let path = workspace.path().to_path_buf();
        fs::write(path.join("artifact"), "test").unwrap();

        workspace.close().unwrap();

        assert!(!path.exists());
        assert!(root.exists());
    }

    #[test]
    fn startup_removes_only_scoped_stale_workspaces() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("builder/work");
        fs::create_dir_all(root.join("vm-build-stale/nested")).unwrap();
        fs::write(root.join("vm-build-stale/nested/artifact"), "test").unwrap();
        fs::create_dir_all(root.join("unrelated")).unwrap();

        prepare_build_work_root(&root).unwrap();

        assert!(!root.join("vm-build-stale").exists());
        assert!(root.join("unrelated").exists());
    }

    #[test]
    fn broad_work_roots_are_rejected() {
        assert!(prepare_build_work_root(Path::new("relative")).is_err());
        assert!(prepare_build_work_root(Path::new("/tmp")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_work_root_is_rejected() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("target");
        let root = parent.path().join("builder/work");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(root.parent().unwrap()).unwrap();
        symlink(target, &root).unwrap();

        assert!(prepare_build_work_root(&root).is_err());
    }
}
