use std::path::Path;

use crate::error::{VmError, VmResult};

use super::guest_runtime::GuestRuntime;

pub(super) fn checkout_root(subject: &GuestRuntime, checkout_id: &str) -> VmResult<String> {
    guest_checkout_root(subject.home(), checkout_id)
}

fn guest_checkout_root(home: &Path, checkout_id: &str) -> VmResult<String> {
    vm_packages::validate_managed_id("checkout ID", checkout_id).map_err(VmError::from)?;
    Ok(home
        .join(".local/share/vm/package-checkouts")
        .join(checkout_id)
        .to_string_lossy()
        .into_owned())
}

pub(super) fn infer_checkout_id(current_dir: &Path, home: &Path) -> VmResult<Option<String>> {
    let root = home.join(".local/share/vm/package-checkouts");
    let Ok(relative) = current_dir.strip_prefix(&root) else {
        return Ok(None);
    };
    let mut components = relative.components();
    let checkout_id = components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .ok_or_else(|| {
            VmError::validation(
                "Managed checkout path has no checkout identity",
                Some("Run the package command from the managed checkout source directory"),
            )
        })?;
    if components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        != Some("source")
    {
        return Err(VmError::validation(
            "Current directory is not inside a managed checkout source directory",
            Some("Run the package command from the managed checkout source directory"),
        ));
    }
    vm_packages::validate_managed_id("checkout ID", checkout_id).map_err(VmError::from)?;
    Ok(Some(checkout_id.to_string()))
}

pub(super) fn create_directory(path: &str) -> VmResult<()> {
    std::fs::create_dir_all(path).map_err(VmError::from)
}

pub(super) fn read_file(path: &str) -> VmResult<String> {
    std::fs::read_to_string(path).map_err(VmError::from)
}

pub(super) fn path_exists(path: &str) -> VmResult<bool> {
    Path::new(path).try_exists().map_err(VmError::from)
}

pub(super) fn path_is_file(path: &str) -> VmResult<bool> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(VmError::from(error)),
    }
}

pub(super) fn remove_file(path: &str) -> VmResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(VmError::from(error)),
    }
}

pub(super) fn remove_directory(path: &str) -> VmResult<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(VmError::from(error)),
    };
    let result = if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(VmError::from(error)),
    }
}

pub(super) fn make_private_executable(path: &str) -> VmResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(VmError::from)?;
    }
    Ok(())
}

pub(super) fn copy_private(content: &[u8], destination: &str) -> VmResult<()> {
    let destination = Path::new(destination);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(VmError::from)?;
    }
    vm_core::file_system::atomic_write(destination, content).map_err(VmError::from)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o600))
            .map_err(VmError::from)?;
    }
    Ok(())
}

pub(super) fn write_checkout_access(
    subject: &GuestRuntime,
    root: &str,
    lease_token: &str,
) -> VmResult<()> {
    copy_private(
        format!("Authorization: Bearer {lease_token}\n").as_bytes(),
        &format!("{root}/authorization-header"),
    )?;
    copy_private(
        format!(
            "{}: {}\n",
            vm_packages::AGENT_CAPABILITY_HEADER,
            subject.agent_token()
        )
        .as_bytes(),
        &format!("{root}/agent-capability-header"),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        create_directory, guest_checkout_root, infer_checkout_id, path_exists, path_is_file,
        read_file, remove_directory, remove_file,
    };
    use std::path::Path;

    #[test]
    fn checkout_roots_cannot_escape_guest_temporary_storage() {
        let home = Path::new("/home/developer");
        assert_eq!(
            guest_checkout_root(home, "pkg-auth-20260811-000001").unwrap(),
            "/home/developer/.local/share/vm/package-checkouts/pkg-auth-20260811-000001"
        );
        for invalid in ["../workspace", "/workspace", "scope/auth", "."] {
            assert!(guest_checkout_root(home, invalid).is_err());
        }
    }

    #[test]
    fn checkout_identity_is_inferred_from_source_or_a_descendant() {
        let home = Path::new("/home/developer");
        for directory in [
            "/home/developer/.local/share/vm/package-checkouts/checkout-123/source",
            "/home/developer/.local/share/vm/package-checkouts/checkout-123/source/src",
        ] {
            assert_eq!(
                infer_checkout_id(Path::new(directory), home).unwrap(),
                Some("checkout-123".into())
            );
        }
        assert_eq!(
            infer_checkout_id(Path::new("/workspace"), home).unwrap(),
            None
        );
    }

    #[test]
    fn guest_file_operations_are_idempotent() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("nested");
        let file = directory.join("state.json");
        let directory = directory.to_str().unwrap();
        let file = file.to_str().unwrap();

        create_directory(directory).unwrap();
        std::fs::write(file, "{}\n").unwrap();
        assert!(path_exists(file).unwrap());
        assert!(path_is_file(file).unwrap());
        assert_eq!(read_file(file).unwrap(), "{}\n");
        remove_file(file).unwrap();
        remove_file(file).unwrap();
        remove_directory(directory).unwrap();
        remove_directory(directory).unwrap();
    }
}
