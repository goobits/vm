use std::{fs, path::Path};

use tracing::info_span;
use vm_core::error::{Result, VmError};
use vm_core::{user_paths, vm_progress, vm_success, vm_warning};

pub(super) fn install(project_root: &Path) -> Result<()> {
    let span = info_span!("install_plugins", operation = "install_plugins");
    let _enter = span.enter();

    vm_progress!("Installing preset plugins...");
    let plugins_dir = project_root
        .parent()
        .ok_or_else(|| VmError::Internal("Could not find project root".to_string()))?
        .join("plugins");
    if !plugins_dir.exists() {
        vm_warning!("Plugins directory not found at {}", plugins_dir.display());
        return Ok(());
    }

    let destination = user_paths::home_dir()?.join(".vm/plugins/presets");
    fs::create_dir_all(&destination).map_err(|error| {
        VmError::Internal(format!("Failed to create plugins directory: {error}"))
    })?;
    let entries = fs::read_dir(&plugins_dir)
        .map_err(|error| VmError::Internal(format!("Failed to read plugins directory: {error}")))?;
    let mut installed = 0;
    for entry in entries {
        let entry = entry.map_err(|error| {
            VmError::Internal(format!("Failed to read directory entry: {error}"))
        })?;
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| path.is_dir() && name.ends_with("-dev"))
        else {
            continue;
        };
        copy_directory(&path, &destination.join(name.trim_end_matches("-dev")))?;
        installed += 1;
    }
    if installed > 0 {
        vm_success!("Installed {installed} preset plugins");
    } else {
        vm_warning!("No plugins found to install");
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination).map_err(|error| {
        VmError::Internal(format!(
            "Failed to create directory {}: {error}",
            destination.display()
        ))
    })?;
    for entry in fs::read_dir(source).map_err(|error| {
        VmError::Internal(format!(
            "Failed to read directory {}: {error}",
            source.display()
        ))
    })? {
        let entry =
            entry.map_err(|error| VmError::Internal(format!("Failed to read entry: {error}")))?;
        let destination_path = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &destination_path)?;
        } else {
            fs::copy(entry.path(), &destination_path).map_err(|error| {
                VmError::Internal(format!(
                    "Failed to copy {} to {}: {error}",
                    entry.path().display(),
                    destination_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::copy_directory;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn directory_copy_preserves_nested_files() {
        let temp_dir = tempdir().expect("create temp directory");
        let source = temp_dir.path().join("source");
        let destination = temp_dir.path().join("destination");
        fs::create_dir_all(source.join("nested")).expect("create source");
        fs::write(source.join("nested/plugin.toml"), "plugin = true").expect("write plugin");
        copy_directory(&source, &destination).expect("copy directory");
        assert_eq!(
            fs::read_to_string(destination.join("nested/plugin.toml")).expect("read plugin"),
            "plugin = true"
        );
    }
}
