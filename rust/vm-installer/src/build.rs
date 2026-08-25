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

fn target_directory() -> PathBuf {
    env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(default_target_directory)
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

    vm_progress!("Cleaning build artifacts...");
    let status = Command::new("cargo")
        .arg("clean")
        .env("CARGO_TARGET_DIR", target_directory())
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

    let target_dir = target_directory();
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
    use super::{default_target_directory, target_directory};

    #[test]
    fn target_directory_honors_configuration_or_uses_the_machine_cache() {
        let target_dir = target_directory();
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
}
