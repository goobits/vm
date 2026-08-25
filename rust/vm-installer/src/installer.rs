use std::{fs, path::Path};

use tracing::info_span;
use vm_core::error::Result;
use vm_core::{user_paths, vm_progress, vm_success};

use crate::{build, completion, platform, plugins};

pub fn install(clean: bool) -> Result<()> {
    let span = info_span!("install", operation = "install", clean);
    let _enter = span.enter();

    let project_root = build::project_root()?;
    let bin_dir = user_paths::user_bin_dir()?;
    if clean {
        build::clean(&project_root)?;
    }

    let source_binary = build::workspace(&project_root)?;
    install_executable(&source_binary, &bin_dir)?;
    plugins::install(&project_root)?;
    platform::ensure_path(&bin_dir)?;
    completion::install(&bin_dir)
}

fn install_executable(source_binary: &Path, bin_dir: &Path) -> Result<()> {
    let span = info_span!(
        "install_executable",
        operation = "install_executable",
        source = %source_binary.display(),
        bin_dir = %bin_dir.display()
    );
    let _enter = span.enter();

    vm_progress!("Creating global 'vm' command...");
    fs::create_dir_all(bin_dir).map_err(|error| {
        vm_core::error::VmError::Internal(format!("Failed to create user bin directory: {error}"))
    })?;

    let link_name = bin_dir.join(vm_platform::platform::executable_name("vm"));
    if link_name.exists() || link_name.is_symlink() {
        fs::remove_file(&link_name).map_err(|error| {
            vm_core::error::VmError::Internal(format!(
                "Failed to remove existing 'vm' file/symlink: {error}"
            ))
        })?;
    }
    vm_platform::current()
        .install_executable(source_binary, bin_dir, "vm")
        .map_err(|error| {
            vm_core::error::VmError::Internal(format!("Failed to install executable: {error}"))
        })?;

    vm_success!(
        "Executable installed: {} -> {}",
        link_name.display(),
        source_binary.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::install_executable;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn executable_installation_replaces_an_existing_link() {
        let temp_dir = tempdir().expect("create temp directory");
        let bin_dir = temp_dir.path().join("bin");
        let source_binary = temp_dir.path().join("vm-binary");
        fs::write(&source_binary, "fake binary content").expect("write source binary");

        install_executable(&source_binary, &bin_dir).expect("install executable");
        install_executable(&source_binary, &bin_dir).expect("replace executable");

        let link_path = bin_dir.join(vm_platform::platform::executable_name("vm"));
        assert!(link_path.exists() || link_path.is_symlink());
    }
}
