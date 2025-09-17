// Standard library
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// External crates
use anyhow::{Context, Result};
use vm_common::{user_paths, vm_success};

// Internal imports
use crate::platform;

pub fn install(clean: bool) -> Result<()> {
    let project_root = get_project_root()?;
    let bin_dir = user_paths::user_bin_dir()?;

    if clean {
        run_cargo_clean(&project_root)?;
    }

    let source_binary = build_workspace(&project_root)?;
    create_symlink(&source_binary, &bin_dir)?;
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

    anyhow::bail!("Could not find project root from executable location. Ensure the project structure is intact.");
}

fn run_cargo_clean(project_root: &Path) -> Result<()> {
    println!("🧹 Cleaning build artifacts...");

    // Clean platform-specific target directory
    let platform = platform::detect_platform_string();
    let target_dir = project_root.join(format!("target-{}", platform));

    let status = Command::new("cargo")
        .arg("clean")
        .env("CARGO_TARGET_DIR", &target_dir)
        .current_dir(project_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to execute 'cargo clean'")?;

    if !status.success() {
        anyhow::bail!("'cargo clean' failed.");
    }
    vm_success!("Build artifacts cleaned.");
    Ok(())
}

fn build_workspace(project_root: &Path) -> Result<PathBuf> {
    println!("🔧 Building Rust binaries...");

    // Use platform-specific target directory to avoid conflicts in shared filesystems
    let platform = platform::detect_platform_string();
    let target_dir = project_root.join(format!("target-{}", platform));

    let status = Command::new("cargo")
        .args(["build", "--release", "--bin", "vm"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .current_dir(project_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to execute 'cargo build'")?;

    if !status.success() {
        anyhow::bail!("Failed to build Rust binaries.");
    }
    vm_success!("Rust binaries built successfully.");

    let binary_path = target_dir.join("release/vm");
    if !binary_path.exists() {
        anyhow::bail!("Could not find compiled 'vm' binary at {:?}", binary_path);
    }
    Ok(binary_path)
}

fn create_symlink(source_binary: &Path, bin_dir: &Path) -> Result<()> {
    println!("🔗 Creating global 'vm' command...");
    fs::create_dir_all(bin_dir).context("Failed to create user bin directory")?;

    let link_name = bin_dir.join("vm");

    // Remove existing link or file if it exists
    if link_name.exists() || link_name.is_symlink() {
        fs::remove_file(&link_name).context("Failed to remove existing 'vm' file/symlink")?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source_binary, &link_name)
            .context("Failed to create symlink")?;
    }
    #[cfg(not(unix))]
    {
        // Add Windows support here if needed in the future
        anyhow::bail!("Automatic symlinking is only supported on Unix-like systems.");
    }

    println!(
        "✅ Symlink created: {} -> {}",
        link_name.display(),
        source_binary.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

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
        let fake_exe = project_root.join("target").join("debug").join("vm-installer");
        fs::create_dir_all(fake_exe.parent().unwrap()).expect("Failed to create target directory");
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
        assert_eq!(found_root.unwrap(), rust_dir);
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
        fs::create_dir_all(binary_path.parent().unwrap()).expect("Failed to create release directory");

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
