use std::fs::{File, OpenOptions};

use crate::error::{VmError, VmResult};

use super::{set_mode, ApplianceFiles};

const MAINTENANCE_LOCK_FILE: &str = "maintenance.lock";

impl ApplianceFiles {
    pub(in crate::commands::packages) fn acquire_maintenance_lock(&self) -> VmResult<File> {
        use fs2::FileExt;

        let file = self.lock_file()?;
        file.try_lock_exclusive().map_err(|_| {
            VmError::validation(
                "Another package backup or restore is already running",
                Some("Wait for it to finish, then retry"),
            )
        })?;
        Ok(file)
    }

    pub(in crate::commands::packages) fn acquire_operation_lock(&self) -> VmResult<File> {
        let file = self.lock_file()?;
        fs2::FileExt::try_lock_shared(&file).map_err(|_| {
            VmError::validation(
                "Package infrastructure maintenance is running",
                Some("Wait for it to finish, then retry"),
            )
        })?;
        Ok(file)
    }

    pub(in crate::commands::packages) fn acquire_lifecycle_lock(&self) -> VmResult<File> {
        let file = self.lock_file()?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| {
            VmError::validation(
                "Another package infrastructure operation is running",
                Some("Wait for it to finish, then retry"),
            )
        })?;
        Ok(file)
    }

    fn lock_file(&self) -> VmResult<File> {
        self.ensure_root()?;
        let path = self.root.join(MAINTENANCE_LOCK_FILE);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| {
                VmError::filesystem(
                    error,
                    path.display().to_string(),
                    "open package maintenance lock",
                )
            })?;
        set_mode(&path, 0o600)?;
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::ApplianceFiles;

    #[test]
    fn maintenance_is_exclusive_while_normal_operations_can_share() {
        let directory = tempfile::tempdir().unwrap();
        let files = ApplianceFiles::at(directory.path().join("packages"));
        let first = files.acquire_operation_lock().unwrap();
        let second = files.acquire_operation_lock().unwrap();
        assert!(files.acquire_maintenance_lock().is_err());

        drop((first, second));
        let maintenance = files.acquire_maintenance_lock().unwrap();
        assert!(files.acquire_operation_lock().is_err());
        drop(maintenance);
    }
}
