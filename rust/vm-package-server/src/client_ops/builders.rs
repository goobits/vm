//! Package builders for different package managers
//!
//! This module provides builders that can create packages from source code
//! for Python, Node.js, and Rust projects.

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::error;
use which::which;

/// Utility for creating consistent progress bars across build operations
pub struct ProgressBarManager {
    pb: ProgressBar,
}

impl ProgressBarManager {
    /// Create a new progress bar with consistent styling
    pub fn new(message: &str) -> Self {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        pb.set_message(message.to_string());
        Self { pb }
    }

    /// Finish the progress bar and clear it
    pub fn finish(self) {
        self.pb.finish_and_clear();
    }
}

/// Helper function to validate that a required tool is available
pub fn ensure_tool_available(tool_name: &str) -> Result<()> {
    if which(tool_name).is_err() {
        error!("{} is not installed or not in PATH", tool_name);
        anyhow::bail!("{} is not installed or not in PATH", tool_name);
    }
    Ok(())
}

/// Trait for package builders that can create build artifacts
pub trait PackageBuilder {
    type Output;

    /// The name of the tool required for building
    fn tool_name(&self) -> &str;

    /// Create the build command with appropriate arguments
    fn build_command(&self) -> Command;

    /// Process the output directory to find build artifacts
    fn process_build_output(&self) -> Result<Self::Output>;

    /// The progress message to show during building
    fn progress_message(&self) -> &str;

    /// Execute the full build process
    fn build(&self) -> Result<Self::Output> {
        ensure_tool_available(self.tool_name())?;

        let pb = ProgressBarManager::new(self.progress_message());

        let output = self
            .build_command()
            .output()
            .with_context(|| format!("Failed to run {} build command", self.tool_name()))?;

        pb.finish();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(tool = %self.tool_name(), stderr = %stderr, "Build command failed");
            anyhow::bail!("Build failed: {}", stderr);
        }

        self.process_build_output()
    }
}

/// Python package builder
pub struct PythonBuilder;

impl PackageBuilder for PythonBuilder {
    type Output = Vec<std::path::PathBuf>;

    fn tool_name(&self) -> &str {
        "python"
    }

    fn build_command(&self) -> Command {
        if Path::new("pyproject.toml").exists() {
            // Try to install build if not available
            let _ = Command::new("python")
                .args(["-m", "pip", "install", "build"])
                .output();

            let mut cmd = Command::new("python");
            cmd.args(["-m", "build"]);
            cmd
        } else {
            let mut cmd = Command::new("python");
            cmd.args(["setup.py", "sdist", "bdist_wheel"]);
            cmd
        }
    }

    fn process_build_output(&self) -> Result<Self::Output> {
        let dist_dir = Path::new("dist");
        if !dist_dir.exists() {
            anyhow::bail!("dist/ directory not found after build");
        }

        let mut package_files = Vec::new();
        for entry in fs::read_dir(dist_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "whl" || ext == "gz" {
                        package_files.push(path);
                    }
                }
            }
        }
        Ok(package_files)
    }

    fn progress_message(&self) -> &str {
        "Building package..."
    }
}

/// NPM package builder
pub struct NpmBuilder;

impl PackageBuilder for NpmBuilder {
    type Output = (std::path::PathBuf, Value);

    fn tool_name(&self) -> &str {
        "npm"
    }

    fn build_command(&self) -> Command {
        let mut cmd = Command::new("npm");
        cmd.args(["pack"]);
        cmd
    }

    fn process_build_output(&self) -> Result<Self::Output> {
        // Read package.json to create metadata
        let package_json = fs::read_to_string("package.json")?;
        let metadata: Value = serde_json::from_str(&package_json)?;

        // Find .tgz files in current directory (created by npm pack)
        let current_dir = std::env::current_dir()?;
        for entry in fs::read_dir(&current_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("tgz")) {
                return Ok((path, metadata));
            }
        }

        anyhow::bail!("No .tgz file found after npm pack");
    }

    fn progress_message(&self) -> &str {
        "Creating package tarball..."
    }
}

/// Cargo package builder
pub struct CargoBuilder;

impl PackageBuilder for CargoBuilder {
    type Output = std::path::PathBuf;

    fn tool_name(&self) -> &str {
        "cargo"
    }

    fn build_command(&self) -> Command {
        let mut cmd = Command::new("cargo");
        cmd.args(["package", "--allow-dirty"]);
        cmd
    }

    fn process_build_output(&self) -> Result<Self::Output> {
        // Try to get the target directory from CARGO_TARGET_DIR or cargo metadata
        let target_dir = if let Ok(output) = Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .output()
        {
            if let Ok(metadata_str) = String::from_utf8(output.stdout) {
                if let Ok(metadata) = serde_json::from_str::<Value>(&metadata_str) {
                    if let Some(target_dir) =
                        metadata.get("target_directory").and_then(|v| v.as_str())
                    {
                        std::path::PathBuf::from(target_dir)
                    } else {
                        std::path::PathBuf::from("target")
                    }
                } else {
                    std::path::PathBuf::from("target")
                }
            } else {
                std::path::PathBuf::from("target")
            }
        } else {
            std::path::PathBuf::from("target")
        };

        let target_package_dir = target_dir.join("package");

        if !target_package_dir.exists() {
            anyhow::bail!(
                "Package directory not found at: {}",
                target_package_dir.display()
            );
        }

        for entry in fs::read_dir(&target_package_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("crate")) {
                return Ok(path);
            }
        }
        anyhow::bail!(
            "Could not find .crate file in {}",
            target_package_dir.display()
        );
    }

    fn progress_message(&self) -> &str {
        "Packaging crate..."
    }
}

/// Build Python package using the builder
pub fn build_python_package() -> Result<Vec<std::path::PathBuf>> {
    PythonBuilder.build()
}

/// Build npm package using the builder
pub fn build_npm_package() -> Result<(std::path::PathBuf, Value)> {
    NpmBuilder.build()
}

/// Build Cargo package using the builder
pub fn build_cargo_package() -> Result<std::path::PathBuf> {
    CargoBuilder.build()
}
