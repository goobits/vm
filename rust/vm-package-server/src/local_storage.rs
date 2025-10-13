use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tracing::{debug, info};

/// List all packages from local storage without requiring server
pub fn list_local_packages(data_dir: &Path) -> Result<HashMap<String, Vec<String>>> {
    let mut packages = HashMap::new();

    // List Python packages
    packages.insert("pypi".to_string(), list_pypi_packages(data_dir)?);

    // List NPM packages
    packages.insert("npm".to_string(), list_npm_packages(data_dir)?);

    // List Cargo crates
    packages.insert("cargo".to_string(), list_cargo_crates(data_dir)?);

    Ok(packages)
}

/// List Python packages from local storage
fn list_pypi_packages(data_dir: &Path) -> Result<Vec<String>> {
    let pypi_dir = data_dir.join("pypi").join("packages");
    let mut packages = HashSet::new();

    if !pypi_dir.exists() {
        debug!("PyPI packages directory does not exist");
        return Ok(Vec::new());
    }

    // Read all files in the packages directory
    for entry in fs::read_dir(pypi_dir)? {
        let entry = entry?;
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        // Extract package name from filename
        // Python packages can be .whl or .tar.gz
        if let Some(package_name) = extract_pypi_package_name(&filename_str) {
            packages.insert(package_name);
        }
    }

    let mut sorted: Vec<String> = packages.into_iter().collect();
    sorted.sort();
    Ok(sorted)
}

/// List NPM packages from local storage
fn list_npm_packages(data_dir: &Path) -> Result<Vec<String>> {
    let npm_metadata_dir = data_dir.join("npm").join("metadata");
    let mut packages = Vec::new();

    if !npm_metadata_dir.exists() {
        debug!("NPM metadata directory does not exist");
        return Ok(Vec::new());
    }

    // Each subdirectory in metadata is a package
    for entry in fs::read_dir(npm_metadata_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let package_name = entry.file_name().to_string_lossy().to_string();
            packages.push(package_name);
        }
    }

    packages.sort();
    Ok(packages)
}

/// List Cargo crates from local storage
fn list_cargo_crates(data_dir: &Path) -> Result<Vec<String>> {
    let cargo_index_dir = data_dir.join("cargo").join("index");
    let mut crates = HashSet::new();

    if !cargo_index_dir.exists() {
        debug!("Cargo index directory does not exist");
        return Ok(Vec::new());
    }

    // Recursively walk the index directory structure
    list_cargo_crates_recursive(&cargo_index_dir, &mut crates)?;

    let mut sorted: Vec<String> = crates.into_iter().collect();
    sorted.sort();
    Ok(sorted)
}

/// Recursively walk Cargo index to find crate names
fn list_cargo_crates_recursive(dir: &Path, crates: &mut HashSet<String>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Recurse into subdirectories
            list_cargo_crates_recursive(&path, crates)?;
        } else if path.is_file() {
            // Each file in the index is a crate
            if let Some(filename) = path.file_name() {
                let crate_name = filename.to_string_lossy().to_string();
                // Skip special files like config.json
                if !crate_name.starts_with('.') && crate_name != "config.json" {
                    crates.insert(crate_name);
                }
            }
        }
    }

    Ok(())
}

/// Extract package name from PyPI filename
fn extract_pypi_package_name(filename: &str) -> Option<String> {
    // Handle wheel files (.whl)
    if filename.ends_with(".whl") {
        // Format: package_name-version-python-etc.whl
        if let Some(dash_pos) = filename.find('-') {
            return Some(filename[..dash_pos].replace('_', "-"));
        }
    }

    // Handle source distributions (.tar.gz)
    if filename.ends_with(".tar.gz") {
        let name_part = filename.trim_end_matches(".tar.gz");
        // Format: package_name-version.tar.gz
        if let Some(dash_pos) = name_part.rfind('-') {
            // Check if what comes after dash looks like a version
            let after_dash = &name_part[dash_pos + 1..];
            if after_dash
                .chars()
                .next()
                .is_some_and(|c| c.is_numeric() || c == 'v')
            {
                return Some(name_part[..dash_pos].to_string());
            }
        }
    }

    None
}

/// Add a Python package to local storage
pub fn add_pypi_package_local(package_file: &Path, data_dir: &Path) -> Result<()> {
    let pypi_dir = data_dir.join("pypi/packages");
    fs::create_dir_all(&pypi_dir)?;

    let filename = package_file
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid package file path"))?;

    let dest_path = pypi_dir.join(filename);
    fs::copy(package_file, &dest_path)?;

    info!(
        source = %package_file.display(),
        dest = %dest_path.display(),
        "Copied Python package to local storage"
    );

    Ok(())
}

/// Add an NPM package to local storage
pub fn add_npm_package_local(tarball_file: &Path, metadata: &Value, data_dir: &Path) -> Result<()> {
    let npm_dir = data_dir.join("npm");
    let tarballs_dir = npm_dir.join("tarballs");
    let metadata_dir = npm_dir.join("metadata");

    fs::create_dir_all(&tarballs_dir)?;
    fs::create_dir_all(&metadata_dir)?;

    // Copy tarball
    let filename = tarball_file
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid tarball file path"))?;
    let dest_tarball = tarballs_dir.join(filename);
    fs::copy(tarball_file, &dest_tarball)?;

    // Save metadata
    let package_name = metadata["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Package name not found in metadata"))?;
    let metadata_file = metadata_dir.join(format!("{package_name}.json"));
    fs::write(&metadata_file, serde_json::to_string_pretty(metadata)?)?;

    info!(
        package = %package_name,
        tarball = %dest_tarball.display(),
        metadata = %metadata_file.display(),
        "Added NPM package to local storage"
    );

    Ok(())
}

/// Add a Cargo crate to local storage
pub fn add_cargo_package_local(crate_file: &Path, data_dir: &Path) -> Result<()> {
    let cargo_dir = data_dir.join("cargo");
    let crates_dir = cargo_dir.join("crates");
    let index_dir = cargo_dir.join("index");

    fs::create_dir_all(&crates_dir)?;
    fs::create_dir_all(&index_dir)?;

    // Copy crate file
    let filename = crate_file
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid crate file path"))?;
    let dest_path = crates_dir.join(filename);
    fs::copy(crate_file, &dest_path)?;

    // Extract crate name from filename (e.g., "hello-world-1.0.0.crate" -> "hello-world")
    let filename_str = filename.to_string_lossy();
    if let Some(crate_name) = crate::utils::extract_cargo_crate_name(&filename_str) {
        // Calculate proper index path using Cargo's index structure (e.g., "co/de/codeflow")
        let index_path_str = crate::cargo::index_path(&crate_name)
            .map_err(|e| anyhow::anyhow!("Failed to calculate index path: {e}"))?;
        let index_file = index_dir.join(&index_path_str);

        // Create parent directories if needed
        if let Some(parent) = index_file.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create or append to index file
        if !index_file.exists() {
            fs::write(&index_file, format!("{{\"name\":\"{crate_name}\"}}\n"))?;
        }

        info!(
            crate_name = %crate_name,
            crate_file = %dest_path.display(),
            index_file = %index_file.display(),
            "Added Cargo crate to local storage"
        );
    }

    Ok(())
}

/// Remove a Python package from local storage
pub fn remove_pypi_package_local(package_name: &str, data_dir: &Path) -> Result<()> {
    let pypi_dir = data_dir.join("pypi/packages");

    if !pypi_dir.exists() {
        return Ok(());
    }

    let mut removed_count = 0;
    for entry in fs::read_dir(&pypi_dir)? {
        let entry = entry?;
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        if let Some(pkg_name) = extract_pypi_package_name(&filename_str) {
            if pkg_name == package_name {
                fs::remove_file(entry.path())?;
                removed_count += 1;
                info!(
                    package = %package_name,
                    file = %filename_str,
                    "Removed Python package from local storage"
                );
            }
        }
    }

    if removed_count == 0 {
        anyhow::bail!("Package '{package_name}' not found in local storage");
    }

    Ok(())
}

/// Remove an NPM package from local storage
pub fn remove_npm_package_local(package_name: &str, data_dir: &Path) -> Result<()> {
    let npm_dir = data_dir.join("npm");
    let tarballs_dir = npm_dir.join("tarballs");
    let metadata_dir = npm_dir.join("metadata");

    // Remove metadata file
    let metadata_file = metadata_dir.join(format!("{package_name}.json"));
    if metadata_file.exists() {
        fs::remove_file(&metadata_file)?;
        info!(
            package = %package_name,
            metadata_file = %metadata_file.display(),
            "Removed NPM package metadata from local storage"
        );
    }

    // Remove tarball files
    if tarballs_dir.exists() {
        let mut removed_count = 0;
        for entry in fs::read_dir(&tarballs_dir)? {
            let entry = entry?;
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            // NPM tarballs are typically named like "package-name-version.tgz"
            if filename_str.starts_with(&format!("{package_name}-"))
                && filename_str.ends_with(".tgz")
            {
                fs::remove_file(entry.path())?;
                removed_count += 1;
                info!(
                    package = %package_name,
                    tarball = %filename_str,
                    "Removed NPM package tarball from local storage"
                );
            }
        }

        if removed_count == 0 && !metadata_file.exists() {
            anyhow::bail!("Package '{package_name}' not found in local storage");
        }
    }

    Ok(())
}

/// Remove a Cargo crate from local storage
pub fn remove_cargo_package_local(crate_name: &str, data_dir: &Path) -> Result<()> {
    let cargo_dir = data_dir.join("cargo");
    let crates_dir = cargo_dir.join("crates");
    let index_dir = cargo_dir.join("index");

    // Remove index entry
    let index_file = index_dir.join(crate_name);
    if index_file.exists() {
        fs::remove_file(&index_file)?;
        info!(
            crate_name = %crate_name,
            index_file = %index_file.display(),
            "Removed Cargo crate index from local storage"
        );
    }

    // Remove crate files
    if crates_dir.exists() {
        let mut removed_count = 0;
        for entry in fs::read_dir(&crates_dir)? {
            let entry = entry?;
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            if let Some(pkg_name) = crate::utils::extract_cargo_crate_name(&filename_str) {
                if pkg_name == crate_name {
                    fs::remove_file(entry.path())?;
                    removed_count += 1;
                    info!(
                        crate_name = %crate_name,
                        crate_file = %filename_str,
                        "Removed Cargo crate file from local storage"
                    );
                }
            }
        }

        if removed_count == 0 && !index_file.exists() {
            anyhow::bail!("Crate '{crate_name}' not found in local storage");
        }
    }

    Ok(())
}
