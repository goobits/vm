use std::{
    fs,
    fs::{File, OpenOptions},
    time::Duration,
};

use crate::error::{VmError, VmResult};

use super::{write_private, ApplianceFiles};

const TOOL_CACHE_LOCK_FILE: &str = "tool-cache.lock";

impl ApplianceFiles {
    pub(in crate::commands::packages) fn acquire_tool_cache_lock(&self) -> VmResult<Option<File>> {
        use fs2::FileExt;

        self.ensure_root()?;
        let path = self.root.join(TOOL_CACHE_LOCK_FILE);
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
                    "open tool catalog cache lock",
                )
            })?;
        vm_core::file_system::set_permissions_mode(&path, 0o600).map_err(VmError::from)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(file)),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(VmError::from(error)),
        }
    }

    pub(in crate::commands::packages) fn read_tool_cache(
        &self,
        name: &str,
        max_age: Duration,
    ) -> VmResult<Option<Vec<u8>>> {
        let path = self.root.join("tool-cache").join(name);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(VmError::from(error)),
        };
        let fresh = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age <= max_age);
        if !fresh {
            return Ok(None);
        }
        fs::read(&path).map(Some).map_err(VmError::from)
    }

    pub(in crate::commands::packages) fn write_tool_cache(
        &self,
        name: &str,
        content: &[u8],
    ) -> VmResult<()> {
        let path = self.root.join("tool-cache").join(name);
        let parent = path.parent().expect("tool cache path has a parent");
        fs::create_dir_all(parent).map_err(VmError::from)?;
        vm_core::file_system::set_permissions_mode(parent, 0o700).map_err(VmError::from)?;
        write_private(&path, content)
    }
}
