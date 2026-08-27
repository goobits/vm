use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

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
    install_source_workspace_marker(&project_root, &bin_dir)?;
    plugins::install(&project_root)?;
    platform::ensure_path(&bin_dir)?;
    completion::install(&bin_dir)
}

fn install_source_workspace_marker(project_root: &Path, bin_dir: &Path) -> Result<()> {
    fs::create_dir_all(bin_dir).map_err(|error| {
        vm_core::error::VmError::Internal(format!(
            "Failed to create user bin directory {}: {error}",
            bin_dir.display()
        ))
    })?;
    let destination = bin_dir.join(vm_core::SOURCE_WORKSPACE_MARKER);
    let (staged_path, mut staged_file) = create_staged_file(&destination)?;
    let result = (|| {
        staged_file.write_all(project_root.to_string_lossy().as_bytes())?;
        staged_file.sync_all()?;
        drop(staged_file);
        replace_file(&staged_path, &destination)
    })();

    if let Err(error) = result {
        let _ = fs::remove_file(&staged_path);
        return Err(vm_core::error::VmError::Internal(format!(
            "Failed to record source workspace at {}: {error}",
            destination.display()
        )));
    }
    Ok(())
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
    copy_executable_atomically(source_binary, &link_name)?;

    vm_success!("Executable installed: {}", link_name.display());
    Ok(())
}

fn copy_executable_atomically(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
        return Err(vm_core::error::VmError::Internal(format!(
            "Built executable not found at {}",
            source.display()
        )));
    }
    let parent = destination.parent().ok_or_else(|| {
        vm_core::error::VmError::Internal(format!(
            "Installed executable has no parent: {}",
            destination.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        vm_core::error::VmError::Internal(format!(
            "Failed to create executable directory {}: {error}",
            parent.display()
        ))
    })?;

    let (staged_path, mut staged_file) = create_staged_file(destination)?;
    let result = (|| {
        let mut source_file = File::open(source)?;
        io::copy(&mut source_file, &mut staged_file)?;
        staged_file.set_permissions(source_file.metadata()?.permissions())?;
        staged_file.sync_all()?;
        drop(staged_file);
        replace_file(&staged_path, destination)
    })();

    if let Err(error) = result {
        let _ = fs::remove_file(&staged_path);
        return Err(vm_core::error::VmError::Internal(format!(
            "Failed to install executable at {}: {error}",
            destination.display()
        )));
    }
    Ok(())
}

fn create_staged_file(destination: &Path) -> Result<(PathBuf, File)> {
    let parent = destination.parent().ok_or_else(|| {
        vm_core::error::VmError::Internal("Installed executable has no parent".to_string())
    })?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("vm");
    for attempt in 0..100_u8 {
        let path = parent.join(format!(".{name}.install-{}-{attempt}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(vm_core::error::VmError::Internal(format!(
                    "Failed to stage executable in {}: {error}",
                    parent.display()
                )))
            }
        }
    }
    Err(vm_core::error::VmError::Internal(format!(
        "Could not reserve an executable staging file in {}",
        parent.display()
    )))
}

#[cfg(unix)]
fn replace_file(staged: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(staged, destination)
}

#[cfg(not(unix))]
fn replace_file(staged: &Path, destination: &Path) -> io::Result<()> {
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(staged, destination)
}

#[cfg(test)]
mod tests {
    use super::{install_executable, install_source_workspace_marker};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn executable_installation_is_a_stable_copy_and_replaces_an_existing_link() {
        let temp_dir = tempdir().expect("create temp directory");
        let bin_dir = temp_dir.path().join("bin");
        let source_binary = temp_dir.path().join("vm-binary");
        fs::write(&source_binary, "fake binary content").expect("write source binary");
        #[cfg(unix)]
        fs::set_permissions(&source_binary, fs::Permissions::from_mode(0o755))
            .expect("make source executable");

        fs::create_dir_all(&bin_dir).expect("create bin directory");
        let legacy_target = temp_dir.path().join("legacy-vm");
        fs::write(&legacy_target, "legacy binary").expect("write legacy target");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&legacy_target, bin_dir.join("vm")).expect("create legacy link");

        install_executable(&source_binary, &bin_dir).expect("install executable");
        fs::write(&source_binary, "replacement binary").expect("replace source binary");
        install_executable(&source_binary, &bin_dir).expect("replace executable");
        fs::remove_file(&source_binary).expect("remove build output");

        let installed_path = bin_dir.join(vm_platform::platform::executable_name("vm"));
        assert_eq!(
            fs::read_to_string(&installed_path).unwrap(),
            "replacement binary"
        );
        assert!(!fs::symlink_metadata(&installed_path)
            .unwrap()
            .file_type()
            .is_symlink());
        #[cfg(unix)]
        assert_ne!(
            fs::metadata(&installed_path).unwrap().permissions().mode() & 0o111,
            0
        );
        assert!(fs::read_dir(&bin_dir).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".vm.install-")
        }));
    }

    #[test]
    fn source_installation_records_workspace_beside_the_stable_executable() {
        let temp_dir = tempdir().expect("create temp directory");
        let bin_dir = temp_dir.path().join("bin");
        let project_root = temp_dir.path().join("checkout");
        fs::create_dir_all(&project_root).expect("create project root");

        install_source_workspace_marker(&project_root, &bin_dir).expect("record source workspace");

        assert_eq!(
            fs::read_to_string(bin_dir.join(vm_core::SOURCE_WORKSPACE_MARKER)).unwrap(),
            project_root.to_string_lossy()
        );
    }
}
