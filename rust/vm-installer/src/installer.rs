// Standard library
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// External crates
use tracing::info_span;
use vm_core::error::Result;
use vm_core::{user_paths, vm_println, vm_progress, vm_success, vm_warning};
use vm_messages::messages::MESSAGES;

// Internal imports
use crate::platform;

pub fn install(clean: bool) -> Result<()> {
    let span = info_span!("install", operation = "install", clean = clean);
    let _enter = span.enter();

    let project_root = get_project_root()?;
    let bin_dir = user_paths::user_bin_dir()?;

    if clean {
        run_cargo_clean(&project_root)?;
    }

    let source_binary = build_workspace(&project_root)?;
    create_symlink(&source_binary, &bin_dir)?;
    install_plugins(&project_root)?;
    platform::ensure_path(&bin_dir)?;

    Ok(())
}

fn get_project_root() -> Result<PathBuf> {
    // Use the executable's path to reliably find the project root, as `cargo run`
    // can change the current working directory.
    let exe_path = env::current_exe()?;

    // Search upwards from the executable's location for the project root,
    // which we identify by the presence of the 'rust/Cargo.toml' file.
    for path in exe_path.ancestors() {
        let rust_cargo_toml = path.join("rust/Cargo.toml");
        if rust_cargo_toml.exists() {
            // The path we need for cargo commands is the 'rust' directory itself.
            return Ok(path.join("rust"));
        }
    }

    Err(vm_core::error::VmError::Internal(
        "Project root not found".to_string(),
    ))
}

fn run_cargo_clean(project_root: &Path) -> Result<()> {
    let platform = platform::detect_platform_string();
    let span = info_span!("cargo_clean",
        operation = "cargo_clean",
        platform = %platform
    );
    let _enter = span.enter();

    vm_progress!("Cleaning build artifacts...");

    // Clean platform-specific target directory
    let target_dir = project_root.join(format!("target-{}", platform));

    let status = Command::new("cargo")
        .arg("clean")
        .env("CARGO_TARGET_DIR", &target_dir)
        .current_dir(project_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to execute 'cargo clean': {}", e))
        })?;

    if !status.success() {
        return Err(vm_core::error::VmError::Internal(format!(
            "Cargo clean failed with exit code: {}",
            status.code().unwrap_or(-1)
        )));
    }
    vm_success!("Build artifacts cleaned.");
    Ok(())
}

fn build_workspace(project_root: &Path) -> Result<PathBuf> {
    let platform = platform::detect_platform_string();
    let span = info_span!("cargo_build",
        operation = "cargo_build",
        platform = %platform,
        target = "vm"
    );
    let _enter = span.enter();

    vm_progress!("Building Rust binaries...");
    vm_println!("{}", MESSAGES.installer_build_time_hint);

    // Check for sccache availability
    let has_sccache = Command::new("sccache")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if has_sccache {
        vm_println!("{}", MESSAGES.installer_sccache_enabled);
    } else {
        vm_warning!("sccache not found - builds will be slower. Install: cargo install sccache");
    }

    // Use platform-specific target directory to avoid conflicts in shared filesystems
    let target_dir = project_root.join(format!("target-{}", platform));

    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--release", "--bin", "vm"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("CARGO_TERM_PROGRESS_WHEN", "always") // Force progress display
        .env("CARGO_TERM_PROGRESS_WIDTH", "80") // Set reasonable width
        .env("CARGO_TERM_COLOR", "always") // Enable colors
        .current_dir(project_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // Enable sccache only if available
    if has_sccache {
        cmd.env("RUSTC_WRAPPER", "sccache");
    }

    let status = cmd.status().map_err(|e| {
        vm_core::error::VmError::Internal(format!("Failed to execute 'cargo build': {}", e))
    })?;

    if !status.success() {
        return Err(vm_core::error::VmError::Internal(format!(
            "Cargo build failed with exit code: {}",
            status.code().unwrap_or(-1)
        )));
    }
    vm_success!("Rust binaries built successfully.");

    let binary_name = vm_platform::platform::executable_name("vm");
    let binary_path = target_dir.join("release").join(&binary_name);
    if !binary_path.exists() {
        return Err(vm_core::error::VmError::Internal(format!(
            "Binary not found at: {}",
            binary_path.display()
        )));
    }
    Ok(binary_path)
}

fn create_symlink(source_binary: &Path, bin_dir: &Path) -> Result<()> {
    let span = info_span!("create_symlink",
        operation = "create_symlink",
        source = %source_binary.display(),
        bin_dir = %bin_dir.display()
    );
    let _enter = span.enter();

    vm_progress!("Creating global 'vm' command...");
    fs::create_dir_all(bin_dir).map_err(|e| {
        vm_core::error::VmError::Internal(format!("Failed to create user bin directory: {}", e))
    })?;

    let executable_name = vm_platform::platform::executable_name("vm");
    let link_name = bin_dir.join(&executable_name);

    // Remove existing link or file if it exists
    if link_name.exists() || link_name.is_symlink() {
        fs::remove_file(&link_name).map_err(|e| {
            vm_core::error::VmError::Internal(format!(
                "Failed to remove existing 'vm' file/symlink: {}",
                e
            ))
        })?;
    }

    // Use platform abstraction for cross-platform installation
    vm_platform::current()
        .install_executable(source_binary, bin_dir, "vm")
        .map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to install executable: {}", e))
        })?;

    vm_success!(
        "Executable installed: {} -> {}",
        link_name.display(),
        source_binary.display()
    );
    Ok(())
}

fn install_plugins(project_root: &Path) -> Result<()> {
    let span = info_span!("install_plugins", operation = "install_plugins");
    let _enter = span.enter();

    vm_progress!("Installing preset plugins...");

    // Get the plugins directory from the repo (go up from rust/ to root)
    let plugins_dir = project_root
        .parent()
        .ok_or_else(|| {
            vm_core::error::VmError::Internal("Could not find project root".to_string())
        })?
        .join("plugins");

    if !plugins_dir.exists() {
        vm_warning!("Plugins directory not found at {}", plugins_dir.display());
        return Ok(());
    }

    // Get user's preset plugins directory
    let user_plugins_dir = user_paths::home_dir()?.join(".vm/plugins/presets");
    fs::create_dir_all(&user_plugins_dir).map_err(|e| {
        vm_core::error::VmError::Internal(format!("Failed to create plugins directory: {}", e))
    })?;

    // Install each plugin from repo
    let entries = fs::read_dir(&plugins_dir).map_err(|e| {
        vm_core::error::VmError::Internal(format!("Failed to read plugins directory: {}", e))
    })?;

    let mut installed_count = 0;
    for entry in entries {
        let entry = entry.map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to read directory entry: {}", e))
        })?;

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip non-plugin directories
        if !dir_name.ends_with("-dev") {
            continue;
        }

        // Extract plugin name (remove -dev suffix)
        let plugin_name = dir_name.trim_end_matches("-dev");
        let dest_dir = user_plugins_dir.join(plugin_name);

        // Copy plugin directory
        copy_dir_recursive(&path, &dest_dir)?;
        installed_count += 1;
    }

    if installed_count > 0 {
        vm_success!("Installed {} preset plugins", installed_count);
    } else {
        vm_warning!("No plugins found to install");
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst).map_err(|e| {
            vm_core::error::VmError::Internal(format!(
                "Failed to create directory {}: {}",
                dst.display(),
                e
            ))
        })?;
    }

    for entry in fs::read_dir(src).map_err(|e| {
        vm_core::error::VmError::Internal(format!(
            "Failed to read directory {}: {}",
            src.display(),
            e
        ))
    })? {
        let entry = entry.map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to read entry: {}", e))
        })?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dst.join(&file_name);

        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path).map_err(|e| {
                vm_core::error::VmError::Internal(format!(
                    "Failed to copy {} to {}: {}",
                    path.display(),
                    dest_path.display(),
                    e
                ))
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_project_root_detection() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let project_root = temp_dir.path().join("test-project");
        let rust_dir = project_root.join("rust");
        fs::create_dir_all(&rust_dir).expect("Failed to create rust directory");

        // Create a Cargo.toml file in the rust directory
        let cargo_toml = rust_dir.join("Cargo.toml");
        fs::write(&cargo_toml, "[package]\nname = \"test\"").expect("Failed to create Cargo.toml");

        // Create a fake executable path within the project structure
        let fake_exe = project_root
            .join("target")
            .join("debug")
            .join("vm-installer");
        fs::create_dir_all(fake_exe.parent().expect("fake_exe should have parent"))
            .expect("Failed to create target directory");
        fs::write(&fake_exe, "fake binary").expect("Failed to create fake binary");

        // Test project root detection logic manually
        // We can't easily test get_project_root directly because it uses current_exe()
        // But we can test the search logic
        let mut found_root = None;
        for path in fake_exe.ancestors() {
            let rust_cargo_toml = path.join("rust/Cargo.toml");
            if rust_cargo_toml.exists() {
                found_root = Some(path.join("rust"));
                break;
            }
        }

        assert!(found_root.is_some());
        assert_eq!(
            found_root.expect("should have found project root"),
            rust_dir
        );
    }

    #[test]
    fn test_symlink_creation_logic() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let bin_dir = temp_dir.path().join("bin");
        let source_binary = temp_dir.path().join("vm-binary");

        // Create fake source binary
        fs::write(&source_binary, "fake binary content").expect("Failed to create source binary");

        // Test symlink creation
        let result = create_symlink(&source_binary, &bin_dir);
        assert!(result.is_ok());

        // Verify bin directory was created
        assert!(bin_dir.exists());

        // Verify symlink was created
        let link_path = bin_dir.join("vm");
        assert!(link_path.exists() || link_path.is_symlink());

        // Test overwriting existing symlink
        let result2 = create_symlink(&source_binary, &bin_dir);
        assert!(result2.is_ok());
    }

    #[test]
    fn test_path_validation() {
        let temp_dir = tempdir().expect("Failed to create temp directory");

        // Test that we can create the expected directory structure
        let bin_dir = temp_dir.path().join(".local").join("bin");
        fs::create_dir_all(&bin_dir).expect("Failed to create .local/bin");
        assert!(bin_dir.exists());

        // Test path existence checking
        assert!(bin_dir.is_dir());

        // Test binary path construction
        let vm_binary = bin_dir.join("vm");
        fs::write(&vm_binary, "test").expect("Failed to create test binary");
        assert!(vm_binary.exists());
    }

    #[test]
    fn test_platform_specific_target_directory() {
        // Test platform string format
        let platform = platform::detect_platform_string();
        assert!(platform.contains('-'));

        // Test target directory path construction
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let project_root = temp_dir.path();
        let target_dir = project_root.join(format!("target-{}", platform));

        // This should be a valid path
        assert!(!target_dir.to_string_lossy().is_empty());
        assert!(target_dir.to_string_lossy().contains(&platform));
    }

    #[test]
    fn test_binary_path_validation() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let target_dir = temp_dir.path().join("target-test");
        let binary_path = target_dir.join("release").join("vm");

        // Create the directory structure
        fs::create_dir_all(
            binary_path
                .parent()
                .expect("binary_path should have parent"),
        )
        .expect("Failed to create release directory");

        // Test binary existence check logic
        assert!(!binary_path.exists()); // Should not exist initially

        // Create the binary
        fs::write(&binary_path, "fake binary").expect("Failed to create binary");
        assert!(binary_path.exists()); // Should exist now

        // Test binary path validation
        assert!(binary_path.is_file());
    }

    #[test]
    fn test_cargo_clean_target_directory() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let platform = "test-platform";
        let target_dir = temp_dir.path().join(format!("target-{}", platform));

        // Create some fake build artifacts
        fs::create_dir_all(&target_dir).expect("Failed to create target directory");
        let artifact = target_dir.join("some-artifact");
        fs::write(&artifact, "build artifact").expect("Failed to create artifact");

        assert!(target_dir.exists());
        assert!(artifact.exists());

        // Test that the target directory path is constructed correctly
        assert!(target_dir.to_string_lossy().contains(platform));
    }
}
