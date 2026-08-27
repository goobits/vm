use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use tracing::info_span;
use vm_core::error::{Result, VmError};
use vm_core::{user_paths, vm_println, vm_progress, vm_success, vm_warning};
use vm_messages::messages::MESSAGES;

use crate::platform;

pub(super) fn project_root() -> Result<PathBuf> {
    let executable = env::current_exe()?;
    let current_directory = env::current_dir()?;
    let root = [&executable, &current_directory]
        .into_iter()
        .flat_map(|start| start.ancestors())
        .find_map(|path| {
            if path.join("Cargo.toml").is_file() && path.join("vm-installer").is_dir() {
                Some(path.to_path_buf())
            } else if path.join("rust/Cargo.toml").is_file() {
                Some(path.join("rust"))
            } else {
                None
            }
        });
    root.ok_or_else(|| VmError::Internal("Project root not found".to_string()))
}

fn configured_target_directory() -> PathBuf {
    env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(default_target_directory)
}

fn target_directory(project_root: &Path) -> Result<PathBuf> {
    let configured = configured_target_directory();
    let target = if configured.exists() {
        fs::canonicalize(&configured).map_err(|error| {
            VmError::filesystem(error, configured.display().to_string(), "canonicalize")
        })?
    } else {
        configured
    };
    if !target.is_absolute() {
        return Err(VmError::validation(
            format!(
                "Cargo target directory must be absolute: {}",
                target.display()
            ),
            None::<String>,
        ));
    }
    let home = user_paths::home_dir()?;
    let home = home.canonicalize().unwrap_or(home);
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    if target == Path::new("/")
        || target == home
        || target == project_root.as_path()
        || project_root.starts_with(&target)
    {
        return Err(VmError::validation(
            format!(
                "Refusing unsafe Cargo target directory: {}",
                target.display()
            ),
            Some("Use a dedicated cache directory such as ~/.cache/vm/cargo-target"),
        ));
    }
    Ok(target)
}

fn default_target_directory() -> PathBuf {
    user_paths::user_cache_dir()
        .map(|cache| cache.join("cargo-target"))
        .unwrap_or_else(|_| PathBuf::from(vm_core::MACHINE_CARGO_TARGET_DIR))
}

pub(super) fn clean(project_root: &Path) -> Result<()> {
    let platform = platform::detect_platform_string();
    let span = info_span!("cargo_clean", operation = "cargo_clean", %platform);
    let _enter = span.enter();

    let target = target_directory(project_root)?;
    verify_clean_target(&target, project_root)?;

    vm_progress!("Cleaning build artifacts...");
    let status = Command::new("cargo")
        .arg("clean")
        .env("CARGO_TARGET_DIR", target)
        .current_dir(project_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| VmError::Internal(format!("Failed to execute 'cargo clean': {error}")))?;
    if !status.success() {
        return Err(VmError::Internal(format!(
            "Cargo clean failed with exit code: {}",
            status.code().unwrap_or(-1)
        )));
    }
    vm_success!("Build artifacts cleaned.");
    Ok(())
}

fn verify_clean_target(target: &Path, project_root: &Path) -> Result<()> {
    let marker = target.join(vm_core::SOURCE_WORKSPACE_MARKER);
    let recorded_root = fs::read_to_string(&marker).map_err(|_| {
        VmError::validation(
            format!(
                "Refusing to clean unmanaged Cargo target directory: {}",
                target.display()
            ),
            Some("Run the installer once without --clean to establish its managed cache"),
        )
    })?;
    let recorded_root = Path::new(recorded_root.trim())
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(recorded_root.trim()));
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    if recorded_root != project_root {
        return Err(VmError::validation(
            format!(
                "Cargo target directory belongs to a different workspace: {}",
                target.display()
            ),
            None::<String>,
        ));
    }
    Ok(())
}

pub(super) fn workspace(project_root: &Path) -> Result<PathBuf> {
    let platform = platform::detect_platform_string();
    let span = info_span!("cargo_build", operation = "cargo_build", %platform, target = "vm");
    let _enter = span.enter();

    vm_progress!("Building Rust binaries...");
    vm_println!("{}", MESSAGES.service.installer_build_time_hint);
    let has_sccache = Command::new("sccache")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if has_sccache {
        vm_println!("{}", MESSAGES.service.installer_sccache_enabled);
    } else {
        vm_warning!("sccache not found - builds will be slower. Install: cargo install sccache");
    }

    let target_dir = target_directory(project_root)?;
    let profile = env::var("VM_INSTALL_PROFILE").unwrap_or_else(|_| "source-install".to_string());
    let mut command = Command::new("cargo");
    command
        .args(["build", "--profile", &profile, "--bin", "vm"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("CARGO_TERM_PROGRESS_WHEN", "always")
        .env("CARGO_TERM_PROGRESS_WIDTH", "80")
        .env("CARGO_TERM_COLOR", "always")
        .current_dir(project_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if has_sccache {
        command.env("RUSTC_WRAPPER", "sccache");
    }
    let status = command
        .status()
        .map_err(|error| VmError::Internal(format!("Failed to execute 'cargo build': {error}")))?;
    if !status.success() {
        return Err(VmError::Internal(format!(
            "Cargo build failed with exit code: {}",
            status.code().unwrap_or(-1)
        )));
    }
    vm_success!("Rust binaries built successfully.");

    let binary = target_dir
        .join(&profile)
        .join(vm_platform::platform::executable_name("vm"));
    if !binary.exists() {
        return Err(VmError::Internal(format!(
            "Binary not found at: {}",
            binary.display()
        )));
    }
    fs::write(
        target_dir.join(vm_core::SOURCE_WORKSPACE_MARKER),
        project_root.to_string_lossy().as_bytes(),
    )
    .map_err(|error| {
        VmError::Internal(format!(
            "Failed to record source workspace in {}: {error}",
            target_dir.display()
        ))
    })?;
    Ok(binary)
}

#[cfg(test)]
mod tests {
    use super::{
        configured_target_directory, default_target_directory, target_directory,
        verify_clean_target,
    };

    #[test]
    fn target_directory_honors_configuration_or_uses_the_machine_cache() {
        let target_dir = configured_target_directory();
        assert!(target_dir.is_absolute());
        if let Some(configured) = std::env::var_os("CARGO_TARGET_DIR") {
            assert_eq!(target_dir, std::path::PathBuf::from(configured));
        } else {
            assert_eq!(target_dir, default_target_directory());
        }
    }

    #[test]
    fn default_target_directory_is_a_managed_user_cache() {
        let target_dir = default_target_directory();
        assert!(target_dir.is_absolute());
        assert!(target_dir.ends_with("cargo-target"));
    }

    #[test]
    fn target_directory_rejects_workspace_ancestors() {
        let project = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("CARGO_TARGET_DIR");
        std::env::set_var("CARGO_TARGET_DIR", project.path());
        let result = target_directory(project.path());
        if let Some(previous) = previous {
            std::env::set_var("CARGO_TARGET_DIR", previous);
        } else {
            std::env::remove_var("CARGO_TARGET_DIR");
        }
        assert!(result.is_err());
    }

    #[test]
    fn clean_requires_a_matching_workspace_marker() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let target = temp.path().join("target");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        assert!(verify_clean_target(&target, &project).is_err());

        std::fs::write(
            target.join(vm_core::SOURCE_WORKSPACE_MARKER),
            temp.path().join("other").to_string_lossy().as_bytes(),
        )
        .unwrap();
        assert!(verify_clean_target(&target, &project).is_err());

        std::fs::write(
            target.join(vm_core::SOURCE_WORKSPACE_MARKER),
            project.to_string_lossy().as_bytes(),
        )
        .unwrap();
        assert!(verify_clean_target(&target, &project).is_ok());
    }
}
