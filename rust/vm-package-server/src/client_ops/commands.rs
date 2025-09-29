//! Main CLI commands for package management
//!
//! This module contains the primary user-facing commands for adding, removing,
//! listing, and managing packages across different package managers.

use super::builders::*;
use super::detection::*;
use crate::api::PackageServerClient;
use anyhow::Result;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};
use std::fs;
use std::io::{self, Write};
use tracing::{debug, error, info, warn};

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

pub fn add_python_package_local(package_name: &str) -> Result<()> {
    info!(package_name = %package_name, "Building Python package for local storage");
    println!("🔨 Building Python package...");

    // Build the package
    let package_files = build_python_package()?;

    // Get data directory (default to ./data)
    let data_dir = std::path::PathBuf::from("./data");

    // Add each built package to local storage
    for package_file in package_files {
        crate::local_storage::add_pypi_package_local(&package_file, &data_dir)?;
    }

    println!("✅ {} successfully added to local storage", package_name);
    Ok(())
}

pub fn add_npm_package_local(package_name: &str) -> Result<()> {
    info!(package_name = %package_name, "Building NPM package for local storage");
    println!("🔨 Building NPM package...");

    // Build the package and get metadata (reuse existing build logic)
    let (tarball_file, metadata) = build_npm_package()?;

    // Get data directory (default to ./data)
    let data_dir = std::path::PathBuf::from("./data");

    // Add to local storage
    crate::local_storage::add_npm_package_local(&tarball_file, &metadata, &data_dir)?;

    // Clean up the tarball file
    let _ = std::fs::remove_file(&tarball_file);

    println!("✅ {} successfully added to local storage", package_name);
    Ok(())
}

pub fn add_cargo_package_local(package_name: &str) -> Result<()> {
    info!(package_name = %package_name, "Building Cargo package for local storage");
    println!("🔨 Building Cargo package...");

    // Build the package (reuse existing build logic)
    let crate_file = build_cargo_package()?;

    // Get data directory (default to ./data)
    let data_dir = std::path::PathBuf::from("./data");

    // Add to local storage
    crate::local_storage::add_cargo_package_local(&crate_file, &data_dir)?;

    println!("✅ {} successfully added to local storage", package_name);
    Ok(())
}

pub fn add_python_package(client: &PackageServerClient, package_name: &str) -> Result<()> {
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

pub fn add_npm_package(client: &PackageServerClient, package_name: &str) -> Result<()> {
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

pub fn add_cargo_package(client: &PackageServerClient, package_name: &str) -> Result<()> {
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
        let packages = crate::local_storage::list_local_packages()?;

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
                    match crate::local_storage::remove_pypi_package_local(&pkg_name, &data_dir) {
                        Ok(_) => println!("✅ Removed Python package: {}", pkg_name),
                        Err(e) => {
                            println!("❌ Failed to remove Python package {}: {}", pkg_name, e)
                        }
                    }
                }
                "npm" => {
                    match crate::local_storage::remove_npm_package_local(&pkg_name, &data_dir) {
                        Ok(_) => println!("✅ Removed NPM package: {}", pkg_name),
                        Err(e) => println!("❌ Failed to remove NPM package {}: {}", pkg_name, e),
                    }
                }
                "cargo" => {
                    match crate::local_storage::remove_cargo_package_local(&pkg_name, &data_dir) {
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
        crate::local_storage::list_local_packages()?
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
        println!("   Hint: Enable this service in vm.yaml for automatic startup (services.package_registry.enabled: true)");
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
