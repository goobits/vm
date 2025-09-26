use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::Value;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use tracing::{debug, error, info, warn};
use which::which;

use vm_package_server::api::PackageServerClient;

/// Utility for creating consistent progress bars across build operations
struct ProgressBarManager {
    pb: ProgressBar,
}

impl ProgressBarManager {
    /// Create a new progress bar with consistent styling
    fn new(message: &str) -> Self {
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
    fn finish(self) {
        self.pb.finish_and_clear();
    }
}

/// Helper function to validate that a required tool is available
fn ensure_tool_available(tool_name: &str) -> Result<()> {
    if which(tool_name).is_err() {
        error!("{} is not installed or not in PATH", tool_name);
        anyhow::bail!("{} is not installed or not in PATH", tool_name);
    }
    Ok(())
}

/// Trait for package builders that can create build artifacts
trait PackageBuilder {
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
struct PythonBuilder;

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
struct NpmBuilder;

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
struct CargoBuilder;

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
        let target_package_dir = Path::new("target/package");
        for entry in fs::read_dir(target_package_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("crate")) {
                return Ok(path);
            }
        }
        anyhow::bail!("Could not find .crate file in target/package/");
    }

    fn progress_message(&self) -> &str {
        "Packaging crate..."
    }
}

/// Package types that can be detected and managed
#[derive(Debug, Clone, PartialEq)]
pub enum PackageType {
    Python,
    Npm,
    Cargo,
}

impl std::fmt::Display for PackageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageType::Python => write!(f, "Python"),
            PackageType::Npm => write!(f, "Node.js"),
            PackageType::Cargo => write!(f, "Rust"),
        }
    }
}

impl PackageType {
    /// Get the package name from the current directory
    pub fn get_package_name(&self) -> Result<String> {
        match self {
            PackageType::Python => get_python_package_name(),
            PackageType::Npm => get_npm_package_name(),
            PackageType::Cargo => get_cargo_package_name(),
        }
    }

    /// Add package locally
    pub fn add_package_local(&self, package_name: &str) -> Result<()> {
        match self {
            PackageType::Python => add_python_package_local(package_name),
            PackageType::Npm => add_npm_package_local(package_name),
            PackageType::Cargo => add_cargo_package_local(package_name),
        }
    }

    /// Add package to server
    pub fn add_package(&self, client: &PackageServerClient, package_name: &str) -> Result<()> {
        match self {
            PackageType::Python => add_python_package(client, package_name),
            PackageType::Npm => add_npm_package(client, package_name),
            PackageType::Cargo => add_cargo_package(client, package_name),
        }
    }

    /// Get package versions from server
    pub fn get_versions(
        &self,
        client: &PackageServerClient,
        package_name: &str,
    ) -> Result<Vec<String>> {
        match self {
            PackageType::Python => client.get_pypi_versions(package_name),
            PackageType::Npm => client.get_npm_versions(package_name),
            PackageType::Cargo => client.get_cargo_versions(package_name),
        }
    }

    /// Delete a specific package version
    pub fn delete_version(
        &self,
        client: &PackageServerClient,
        package_name: &str,
        version: &str,
        force_delete: bool,
    ) -> Result<()> {
        match self {
            PackageType::Python => client.delete_pypi_version(package_name, version),
            PackageType::Npm => client.delete_npm_version(package_name, version),
            PackageType::Cargo => client.delete_cargo_version(package_name, version, force_delete),
        }
    }

    /// Delete entire package
    pub fn delete_package(&self, client: &PackageServerClient, package_name: &str) -> Result<()> {
        match self {
            PackageType::Python => client.delete_pypi_package(package_name),
            PackageType::Npm => client.delete_npm_package(package_name),
            PackageType::Cargo => client.delete_cargo_crate(package_name),
        }
    }

    /// Remove package from local storage
    pub fn remove_package_local(
        &self,
        package_name: &str,
        data_dir: &std::path::Path,
    ) -> Result<()> {
        match self {
            PackageType::Python => {
                vm_package_server::local_storage::remove_pypi_package_local(package_name, data_dir)
            }
            PackageType::Npm => {
                vm_package_server::local_storage::remove_npm_package_local(package_name, data_dir)
            }
            PackageType::Cargo => {
                vm_package_server::local_storage::remove_cargo_package_local(package_name, data_dir)
            }
        }
    }

    /// Get action description for deletion
    pub fn delete_action_description(&self, delete_all: bool, force_delete: bool) -> &str {
        match (self, delete_all, force_delete) {
            (_, true, _) => "delete ALL versions of",
            (PackageType::Cargo, false, false) => "yank version",
            (PackageType::Cargo, false, true) => "force delete version",
            (PackageType::Npm, false, _) => "unpublish version",
            (PackageType::Python, false, _) => "delete version",
        }
    }

    /// Create from index (for UI)
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => PackageType::Python,
            1 => PackageType::Npm,
            2 => PackageType::Cargo,
            _ => unreachable!(),
        }
    }
}

/// Package information containing type and name
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub package_type: PackageType,
    pub name: String,
}

/// Detect package type in current directory
#[allow(dead_code)]
pub fn detect_package_type() -> Result<PackageType> {
    let package_types = detect_package_types()?;
    if package_types.is_empty() {
        anyhow::bail!(
            "No package detected in current directory.\n\
            Supported: Python (setup.py/pyproject.toml), NPM (package.json), Cargo (Cargo.toml)"
        );
    }
    Ok(package_types[0].clone())
}

/// Detect all package types in current directory
pub fn detect_package_types() -> Result<Vec<PackageType>> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let mut detected_types = Vec::new();

    // Check for Python packages
    if current_dir.join("setup.py").exists() || current_dir.join("pyproject.toml").exists() {
        detected_types.push(PackageType::Python);
    }

    // Check for Node.js packages
    if current_dir.join("package.json").exists() {
        detected_types.push(PackageType::Npm);
    }

    // Check for Rust packages
    if current_dir.join("Cargo.toml").exists() {
        detected_types.push(PackageType::Cargo);
    }

    Ok(detected_types)
}

/// Get package name from current directory
pub fn get_package_name(package_type: &PackageType) -> Result<String> {
    package_type.get_package_name()
}

fn get_python_package_name() -> Result<String> {
    // Try pyproject.toml first
    if Path::new("pyproject.toml").exists() {
        let content = fs::read_to_string("pyproject.toml")?;
        if let Some(line) = content.lines().find(|line| line.trim().starts_with("name")) {
            if let Some(name) = line.split('=').nth(1) {
                return Ok(name.trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }
    }

    // Fall back to setup.py
    if Path::new("setup.py").exists() {
        let output = Command::new("python")
            .args(["setup.py", "--name"])
            .output()
            .context("Failed to get package name from setup.py")?;

        if output.status.success() {
            let name = String::from_utf8(output.stdout)?.trim().to_string();
            if !name.is_empty() {
                return Ok(name);
            }
        }
    }

    anyhow::bail!("Could not determine Python package name");
}

fn get_npm_package_name() -> Result<String> {
    let package_json = fs::read_to_string("package.json").context("Failed to read package.json")?;

    let parsed: Value =
        serde_json::from_str(&package_json).context("Failed to parse package.json")?;

    parsed["name"]
        .as_str()
        .map(|s| s.to_string())
        .context("No 'name' field found in package.json")
}

fn get_cargo_package_name() -> Result<String> {
    let cargo_toml = fs::read_to_string("Cargo.toml").context("Failed to read Cargo.toml")?;

    // Simple toml parsing - look for name = "..." in [package] section
    let mut in_package_section = false;
    for line in cargo_toml.lines() {
        let line = line.trim();
        if line == "[package]" {
            in_package_section = true;
            continue;
        }
        if line.starts_with('[') && line != "[package]" {
            in_package_section = false;
            continue;
        }
        if in_package_section && line.starts_with("name") {
            if let Some(name_part) = line.split('=').nth(1) {
                let name = name_part.trim().trim_matches('"').trim_matches('\'');
                if !name.is_empty() {
                    return Ok(name.to_string());
                }
            }
        }
    }

    anyhow::bail!("Could not find package name in Cargo.toml");
}

/// Add/publish package from current directory
pub fn add_package(server_url: &str, type_filter: Option<&str>) -> Result<()> {
    let client = PackageServerClient::new(server_url);

    // Check if server is running, if not use local storage
    let use_local = !client.is_server_running();
    if use_local {
        info!("Server not running, adding packages to local storage");
        println!("ℹ️  Server not running, adding packages to local storage");
        println!();
    }

    // Detect all package types
    let detected_types = detect_package_types()?;
    if detected_types.is_empty() {
        anyhow::bail!(
            "No package detected in current directory.\n\
            Supported: Python (setup.py/pyproject.toml), NPM (package.json), Cargo (Cargo.toml)"
        );
    }

    // Filter types if specified via CLI
    let selected_types = if let Some(type_filter) = type_filter {
        filter_types_from_string(&detected_types, type_filter)?
    } else if detected_types.len() == 1 {
        // Single package type - use it directly
        detected_types
    } else {
        // Multiple package types - ask user to select
        select_package_types_interactive(&detected_types)?
    };

    // Build packages with their names
    let mut packages_to_build = Vec::new();
    for package_type in &selected_types {
        let package_name = get_package_name(package_type)?;
        packages_to_build.push(PackageInfo {
            package_type: package_type.clone(),
            name: package_name,
        });
    }

    // Display what we're going to build
    if packages_to_build.len() == 1 {
        let pkg = &packages_to_build[0];
        info!(package_type = ?pkg.package_type, package_name = %pkg.name, "Adding package to server");
        println!("📦 {} package detected: {}", pkg.package_type, pkg.name);
    } else {
        println!("📦 Multiple packages detected:");
        for pkg in &packages_to_build {
            println!("  • {} ({})", pkg.name, pkg.package_type);
        }
        println!();
    }

    // Build and upload each package
    let mut results = Vec::new();
    for (i, package_info) in packages_to_build.iter().enumerate() {
        if packages_to_build.len() > 1 {
            println!(
                "[{}/{}] Building {} package: {}",
                i + 1,
                packages_to_build.len(),
                package_info.package_type,
                package_info.name
            );
        }

        let result = if use_local {
            package_info
                .package_type
                .add_package_local(&package_info.name)
        } else {
            package_info
                .package_type
                .add_package(&client, &package_info.name)
        };

        match result {
            Ok(_) => {
                results.push((package_info.clone(), true));
                if packages_to_build.len() > 1 {
                    println!("  ✅ {} successfully published", package_info.name);
                }
            }
            Err(e) => {
                results.push((package_info.clone(), false));
                error!(package_name = %package_info.name, error = %e, "Failed to publish package");
                if packages_to_build.len() > 1 {
                    println!("  ❌ {} failed to publish: {}", package_info.name, e);
                } else {
                    return Err(e);
                }
            }
        }
    }

    // Summary for multiple packages
    if packages_to_build.len() > 1 {
        println!();
        println!("📋 Publishing Summary:");
        let successful = results.iter().filter(|(_, success)| *success).count();
        let failed = results.len() - successful;

        for (package_info, success) in &results {
            let status = if *success { "✅" } else { "❌" };
            println!(
                "  {} {} ({})",
                status, package_info.name, package_info.package_type
            );
        }

        println!();
        if failed > 0 {
            println!(
                "❌ {} packages failed, {} packages succeeded",
                failed, successful
            );
            anyhow::bail!("Some packages failed to publish");
        } else {
            println!("✨ All {} packages published successfully!", successful);
        }
    }

    Ok(())
}

/// Filter package types based on CLI string input
fn filter_types_from_string(
    detected_types: &[PackageType],
    type_filter: &str,
) -> Result<Vec<PackageType>> {
    let lowercase_types: Vec<String> = type_filter
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .collect();
    let requested_types: Vec<&str> = lowercase_types.iter().map(|s| s.as_str()).collect();
    let mut selected_types = Vec::new();

    for requested in requested_types {
        let package_type = match requested {
            "python" | "py" => PackageType::Python,
            "npm" | "node" | "nodejs" | "javascript" | "js" => PackageType::Npm,
            "cargo" | "rust" | "rs" => PackageType::Cargo,
            _ => {
                anyhow::bail!(
                    "Unknown package type '{}'. Supported: python, npm, cargo",
                    requested
                );
            }
        };

        if detected_types.contains(&package_type) {
            if !selected_types.contains(&package_type) {
                selected_types.push(package_type);
            }
        } else {
            anyhow::bail!(
                "Package type '{}' not found in current directory",
                requested
            );
        }
    }

    if selected_types.is_empty() {
        anyhow::bail!("No matching package types found");
    }

    Ok(selected_types)
}

/// Interactive selection for multiple package types
fn select_package_types_interactive(detected_types: &[PackageType]) -> Result<Vec<PackageType>> {
    let options: Vec<String> = detected_types
        .iter()
        .map(|t| format!("{} package", t))
        .collect();

    println!("🔍 Multiple package types detected in current directory:");
    for (i, package_type) in detected_types.iter().enumerate() {
        println!("  {}. {} package", i + 1, package_type);
    }
    println!();

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select which package types to publish (Space to select, Enter to confirm)")
        .items(&options)
        .defaults(&vec![true; detected_types.len()]) // Select all by default
        .interact()?;

    if selections.is_empty() {
        anyhow::bail!("No package types selected");
    }

    let selected_types: Vec<PackageType> = selections
        .into_iter()
        .map(|i| detected_types[i].clone())
        .collect();

    Ok(selected_types)
}

fn build_python_package() -> Result<Vec<std::path::PathBuf>> {
    PythonBuilder.build()
}

fn build_npm_package() -> Result<(std::path::PathBuf, Value)> {
    NpmBuilder.build()
}

fn build_cargo_package() -> Result<std::path::PathBuf> {
    CargoBuilder.build()
}

fn add_python_package_local(package_name: &str) -> Result<()> {
    info!(package_name = %package_name, "Building Python package for local storage");
    println!("🔨 Building Python package...");

    // Build the package
    let package_files = build_python_package()?;

    // Get data directory (default to ./data)
    let data_dir = std::path::PathBuf::from("./data");

    // Add each built package to local storage
    for package_file in package_files {
        vm_package_server::local_storage::add_pypi_package_local(&package_file, &data_dir)?;
    }

    println!("✅ {} successfully added to local storage", package_name);
    Ok(())
}

fn add_npm_package_local(package_name: &str) -> Result<()> {
    info!(package_name = %package_name, "Building NPM package for local storage");
    println!("🔨 Building NPM package...");

    // Build the package and get metadata (reuse existing build logic)
    let (tarball_file, metadata) = build_npm_package()?;

    // Get data directory (default to ./data)
    let data_dir = std::path::PathBuf::from("./data");

    // Add to local storage
    vm_package_server::local_storage::add_npm_package_local(&tarball_file, &metadata, &data_dir)?;

    // Clean up the tarball file
    let _ = std::fs::remove_file(&tarball_file);

    println!("✅ {} successfully added to local storage", package_name);
    Ok(())
}

fn add_cargo_package_local(package_name: &str) -> Result<()> {
    info!(package_name = %package_name, "Building Cargo package for local storage");
    println!("🔨 Building Cargo package...");

    // Build the package (reuse existing build logic)
    let crate_file = build_cargo_package()?;

    // Get data directory (default to ./data)
    let data_dir = std::path::PathBuf::from("./data");

    // Add to local storage
    vm_package_server::local_storage::add_cargo_package_local(&crate_file, &data_dir)?;

    println!("✅ {} successfully added to local storage", package_name);
    Ok(())
}

fn add_python_package(client: &PackageServerClient, package_name: &str) -> Result<()> {
    info!(package_name = %package_name, "Building Python package");
    println!("🔨 Building Python package...");

    // Build the package using the common builder
    let package_files = PythonBuilder.build()?;

    info!("Uploading Python package to server");
    println!("📤 Uploading to package server...");

    // Upload all built files
    for path in package_files {
        debug!(file = %path.display(), "Uploading Python package file");
        client.upload_pypi_package(&path)?;
    }

    info!(package_name = %package_name, "Python package uploaded successfully");
    println!("📋 Install with: pip install {}", package_name);
    Ok(())
}

fn add_npm_package(client: &PackageServerClient, package_name: &str) -> Result<()> {
    info!(package_name = %package_name, "Building NPM package");
    println!("🔨 Building NPM package...");

    // Build the package using the common builder
    let (tarball_path, metadata) = NpmBuilder.build()?;

    info!("Publishing NPM package to server");
    println!("📤 Publishing to package server...");

    // Read tarball data for upload
    let tarball_data = fs::read(&tarball_path)?;
    client.upload_npm_package(package_name, &tarball_data, metadata)?;

    // Clean up tarball
    let _ = fs::remove_file(&tarball_path);

    info!(package_name = %package_name, "NPM package published successfully");
    println!("📋 Install with: npm install {}", package_name);
    Ok(())
}

fn add_cargo_package(client: &PackageServerClient, package_name: &str) -> Result<()> {
    info!(package_name = %package_name, "Building Cargo package");
    println!("🔨 Building Cargo package...");

    // Build the package using the common builder
    let crate_file = CargoBuilder.build()?;

    info!("Publishing Cargo crate to server");
    println!("📤 Publishing to package server...");
    client.upload_cargo_crate(&crate_file)?;

    info!(package_name = %package_name, "Cargo crate published successfully");
    println!("📋 Install with: cargo add {}", package_name);
    Ok(())
}

fn remove_package_local(force: bool) -> Result<()> {
    let data_dir = std::path::PathBuf::from("./data");

    if force {
        println!("🔍 Listing available packages from local storage...");

        // List all packages from local storage
        let packages = vm_package_server::local_storage::list_local_packages()?;

        let mut all_packages = Vec::new();
        for (package_type, package_list) in &packages {
            for package_name in package_list {
                all_packages.push((package_type.clone(), package_name.clone()));
            }
        }

        if all_packages.is_empty() {
            println!("📦 No packages found in local storage");
            return Ok(());
        }

        println!("📦 Available packages:");
        for (i, (pkg_type, pkg_name)) in all_packages.iter().enumerate() {
            println!("  {}. {} ({})", i + 1, pkg_name, pkg_type);
        }
        println!();

        print!("Enter package numbers to delete (comma-separated) or 'all': ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        let packages_to_delete = if input == "all" {
            all_packages
        } else {
            let mut selected = Vec::new();
            for num_str in input.split(',') {
                if let Ok(num) = num_str.trim().parse::<usize>() {
                    if num > 0 && num <= all_packages.len() {
                        selected.push(all_packages[num - 1].clone());
                    }
                }
            }
            selected
        };

        if packages_to_delete.is_empty() {
            println!("❌ No valid packages selected");
            return Ok(());
        }

        for (pkg_type, pkg_name) in packages_to_delete {
            match pkg_type.as_str() {
                "pypi" => {
                    match vm_package_server::local_storage::remove_pypi_package_local(
                        &pkg_name, &data_dir,
                    ) {
                        Ok(_) => println!("✅ Removed Python package: {}", pkg_name),
                        Err(e) => {
                            println!("❌ Failed to remove Python package {}: {}", pkg_name, e)
                        }
                    }
                }
                "npm" => {
                    match vm_package_server::local_storage::remove_npm_package_local(
                        &pkg_name, &data_dir,
                    ) {
                        Ok(_) => println!("✅ Removed NPM package: {}", pkg_name),
                        Err(e) => println!("❌ Failed to remove NPM package {}: {}", pkg_name, e),
                    }
                }
                "cargo" => {
                    match vm_package_server::local_storage::remove_cargo_package_local(
                        &pkg_name, &data_dir,
                    ) {
                        Ok(_) => println!("✅ Removed Cargo crate: {}", pkg_name),
                        Err(e) => println!("❌ Failed to remove Cargo crate {}: {}", pkg_name, e),
                    }
                }
                _ => println!("❌ Unknown package type: {}", pkg_type),
            }
        }
    } else {
        // Interactive mode for local packages
        println!("🔍 Scanning current directory for packages to remove...");

        let detected_types = detect_package_types()?;
        if detected_types.is_empty() {
            anyhow::bail!(
                "No package detected in current directory.\n\
                Use --force to select from all available packages."
            );
        }

        for package_type in detected_types {
            let package_name = get_package_name(&package_type)?;

            match package_type.remove_package_local(&package_name, &data_dir) {
                Ok(_) => println!("✅ Removed {} package: {}", package_type, package_name),
                Err(e) => println!(
                    "❌ Failed to remove {} package {}: {}",
                    package_type, package_name, e
                ),
            }
        }
    }

    Ok(())
}

/// Remove package from server with interactive prompts or forced removal
pub fn remove_package(server_url: &str, force: bool) -> Result<()> {
    let client = PackageServerClient::new(server_url);

    // Check if server is running, if not use local storage
    let use_local = !client.is_server_running();
    if use_local {
        info!("Server not running, removing packages from local storage");
        println!("ℹ️  Server not running, removing packages from local storage");
        println!();
        return remove_package_local(force);
    }

    // If force mode is enabled, we need to list packages and let user specify which to delete
    if force {
        println!("🔍 Listing available packages...");

        // List all packages
        let packages = client.get_all_packages()?;

        println!("Available packages:");
        if let Some(cargo_packages) = packages.get("cargo").and_then(|p| p.as_array()) {
            for package in cargo_packages {
                println!("  cargo: {}", package.as_str().unwrap_or("unknown"));
            }
        }
        if let Some(pypi_packages) = packages.get("pypi").and_then(|p| p.as_array()) {
            for package in pypi_packages {
                println!("  pypi: {}", package.as_str().unwrap_or("unknown"));
            }
        }
        if let Some(npm_packages) = packages.get("npm").and_then(|p| p.as_array()) {
            for package in npm_packages {
                println!("  npm: {}", package.as_str().unwrap_or("unknown"));
            }
        }

        println!("❌ Force mode requires specifying package details directly.");
        println!("Use the interactive mode (without --force) to select packages to remove.");
        return Ok(());
    }

    // Select package type
    let package_types = vec!["Python (PyPI)", "JavaScript (NPM)", "Rust (Cargo)"];

    let package_type_idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select package type")
        .items(&package_types)
        .default(0)
        .interact()?;

    let package_type = PackageType::from_index(package_type_idx);

    // Get package name
    let package_name: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter package name")
        .interact_text()?;

    // Fetch versions
    info!(package_name = %package_name, "Fetching package versions");
    println!("📋 Fetching versions for {}...", package_name);

    let versions = package_type.get_versions(&client, &package_name)?;

    if versions.is_empty() {
        warn!(package_name = %package_name, "No versions found for package");
        println!("❌ No versions found for package '{}'", package_name);
        println!("   The package may not exist or the server may not have any versions.");
        return Ok(());
    }

    // Build selection list with "ALL VERSIONS" option
    let mut version_choices = vec!["🗑️  ALL VERSIONS (delete entire package)".to_string()];
    for (i, version) in versions.iter().enumerate() {
        if i == 0 {
            version_choices.push(format!("{}  {} (latest)", "📦", version));
        } else {
            version_choices.push(format!("{}  {}", "📦", version));
        }
    }

    // Select version
    let version_idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select version to remove")
        .items(&version_choices)
        .default(0)
        .interact()?;

    let delete_all = version_idx == 0;
    let selected_version = if delete_all {
        None
    } else {
        Some(versions[version_idx - 1].clone())
    };

    // For Cargo, ask about yank vs force delete
    let force_delete =
        if package_type == PackageType::Cargo && !delete_all && selected_version.is_some() {
            let deletion_types = vec![
                "Yank (recommended - soft delete, can be undone)",
                "Force Delete (permanent removal, cannot be undone)",
            ];

            let deletion_type_idx = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select deletion type")
                .items(&deletion_types)
                .default(0)
                .interact()?;

            deletion_type_idx == 1
        } else {
            false
        };

    // Build confirmation message
    let action = package_type.delete_action_description(delete_all, force_delete);

    let confirm_msg = if let Some(version) = &selected_version {
        format!(
            "Are you sure you want to {} {} of '{}'?",
            action.red().bold(),
            version.yellow(),
            package_name.cyan()
        )
    } else {
        format!(
            "⚠️  Are you sure you want to {} '{}'? This will remove ALL versions!",
            action.red().bold(),
            package_name.cyan()
        )
    };

    // Final confirmation
    if !Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(&confirm_msg)
        .default(false)
        .interact()?
    {
        info!("Package deletion cancelled by user");
        println!("❌ Operation cancelled");
        return Ok(());
    }

    // Execute deletion
    if let Some(version) = selected_version {
        info!(package_name = %package_name, version = %version, "Deleting package version");
        package_type.delete_version(&client, &package_name, &version, force_delete)?;
    } else {
        info!(package_name = %package_name, "Deleting all package versions");
        package_type.delete_package(&client, &package_name)?;
    }

    info!("Package deletion completed successfully");
    println!("✨ Operation completed successfully!");
    Ok(())
}

/// List all packages on the server
pub fn list_packages(server_url: &str) -> Result<()> {
    let client = PackageServerClient::new(server_url);

    // Try to get packages from server first, fallback to local storage if server is not running
    let packages = if client.is_server_running() {
        info!(server_url = %server_url, "Fetching packages from server");
        client.list_all_packages()?
    } else {
        info!("Server not running, reading packages from local storage");
        println!("ℹ️  Server not running, listing packages from local storage");
        println!();
        vm_package_server::local_storage::list_local_packages()?
    };

    info!("Listing all packages");
    println!("📦 Packages:");
    println!();

    if let Some(pypi_packages) = packages.get("pypi") {
        println!("🐍 Python packages:");
        if pypi_packages.is_empty() {
            println!("  None");
        } else {
            for pkg in pypi_packages {
                println!("  • {}", pkg);
            }
        }
        println!();
    }

    if let Some(npm_packages) = packages.get("npm") {
        println!("📦 NPM packages:");
        if npm_packages.is_empty() {
            println!("  None");
        } else {
            for pkg in npm_packages {
                println!("  • {}", pkg);
            }
        }
        println!();
    }

    if let Some(cargo_packages) = packages.get("cargo") {
        println!("🦀 Cargo crates:");
        if cargo_packages.is_empty() {
            println!("  None");
        } else {
            for pkg in cargo_packages {
                println!("  • {}", pkg);
            }
        }
        println!();
    }

    Ok(())
}

/// Show server status
pub fn show_status(server_url: &str) -> Result<()> {
    let client = PackageServerClient::new(server_url);

    if !client.is_server_running() {
        warn!(server_url = %server_url, "Package server is not running");
        println!("❌ Package server is not running at {}", server_url);
        println!("   Start it with: pkg-server start");
        return Ok(());
    }

    info!(server_url = %server_url, "Package server is running");
    println!("✅ Package server is running at {}", server_url);

    if let Ok(status) = client.get_server_status() {
        debug!(status = ?status, "Retrieved server status");
        println!("📊 Server status: {}", status);
    }

    // Also show package counts
    if let Ok(packages) = client.list_all_packages() {
        let pypi_count = packages.get("pypi").map(|p| p.len()).unwrap_or(0);
        let npm_count = packages.get("npm").map(|p| p.len()).unwrap_or(0);
        let cargo_count = packages.get("cargo").map(|p| p.len()).unwrap_or(0);
        let total = pypi_count + npm_count + cargo_count;

        println!();
        println!("📈 Package counts:");
        println!("  Python: {}", pypi_count);
        println!("  NPM:    {}", npm_count);
        println!("  Cargo:  {}", cargo_count);
        println!("  Total:  {}", total);
    }

    Ok(())
}
