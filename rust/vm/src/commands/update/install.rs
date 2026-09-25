use crate::error::VmError;
use std::io::Write;
use std::path::Path;

pub(super) fn install_executable_update(
    source: &Path,
    current: &Path,
    version: &str,
) -> Result<(), VmError> {
    let parent = current.parent().ok_or_else(|| {
        VmError::validation("Current executable has no parent directory", None::<String>)
    })?;
    let mut staged = tempfile::Builder::new()
        .prefix(".vm-update-")
        .tempfile_in(parent)?;
    let mut source = std::fs::File::open(source)?;
    std::io::copy(&mut source, staged.as_file_mut())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))?;
    }
    staged.as_file_mut().flush()?;
    staged.as_file().sync_all()?;
    let staged = staged.into_temp_path();

    // Windows cannot replace the running executable directly. Reserve a
    // random sibling path for its short-lived rollback file instead of using a
    // predictable `.backup` name. Unix ignores this path and renames atomically.
    let backup = tempfile::Builder::new()
        .prefix(".vm-backup-")
        .tempfile_in(parent)?
        .into_temp_path();
    #[cfg(unix)]
    std::fs::copy(current, &backup)?;
    replace_executable(staged.as_ref(), current, backup.as_ref())?;
    if let Err(error) = vm_core::install_record::write(current, version) {
        rollback_executable(backup.as_ref(), current).map_err(|rollback| {
            VmError::validation(
                format!(
                    "Version record failed ({error}) and executable rollback failed ({rollback})"
                ),
                Some("Restore the installation with the VM installer"),
            )
        })?;
        return Err(VmError::general(
            error,
            "Version record failed; previous executable restored",
        ));
    }
    Ok(())
}

fn rollback_executable(backup: &Path, current: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    std::fs::remove_file(current)?;
    std::fs::rename(backup, current)
}

pub(super) fn replace_executable(
    staged: &std::path::Path,
    current: &std::path::Path,
    _backup: &std::path::Path,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::rename(staged, current)
    }

    #[cfg(windows)]
    {
        let _ = std::fs::remove_file(_backup);
        std::fs::rename(current, _backup)?;
        if let Err(error) = std::fs::rename(staged, current) {
            let _ = std::fs::rename(_backup, current);
            return Err(error);
        }
        Ok(())
    }
}
