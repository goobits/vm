// Standard library
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// External crates
use anyhow::{Context, Result};
use vm_common::vm_success;

// Internal imports
use crate::platform;

const BIN_DIR_NAME: &str = ".local/bin";

pub fn install(clean: bool) -> Result<()> {
    let project_root = get_project_root()?;
    let bin_dir = dirs::home_dir()
        .context("Could not find home directory")?
        .join(BIN_DIR_NAME);

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
    fs::create_dir_all(bin_dir).context("Failed to create ~/.local/bin directory")?;

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
