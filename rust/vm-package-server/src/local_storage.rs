use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::Result;
use tracing::debug;

/// Read the registry inventory without mutating its private storage.
pub fn list_local_packages(data_dir: &Path) -> Result<HashMap<String, Vec<String>>> {
    let mut packages = HashMap::new();
    packages.insert("pypi".to_string(), list_pypi_packages(data_dir)?);
    packages.insert("npm".to_string(), list_npm_packages(data_dir)?);
    packages.insert("cargo".to_string(), list_cargo_crates(data_dir)?);
    Ok(packages)
}

fn list_pypi_packages(data_dir: &Path) -> Result<Vec<String>> {
    let packages_dir = data_dir.join("pypi/packages");
    if !packages_dir.exists() {
        debug!("PyPI packages directory does not exist");
        return Ok(Vec::new());
    }

    let mut package_names = HashSet::new();
    for entry in fs::read_dir(packages_dir)? {
        let entry = entry?;
        if let Some(name) = extract_pypi_package_name(&entry.file_name().to_string_lossy()) {
            package_names.insert(name);
        }
    }
    let mut packages = package_names.into_iter().collect::<Vec<_>>();
    packages.sort();
    Ok(packages)
}

fn list_npm_packages(data_dir: &Path) -> Result<Vec<String>> {
    let metadata_dir = data_dir.join("npm/metadata");
    if !metadata_dir.exists() {
        debug!("npm metadata directory does not exist");
        return Ok(Vec::new());
    }

    let mut packages = Vec::new();
    for entry in fs::read_dir(metadata_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            packages.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    packages.sort();
    Ok(packages)
}

fn list_cargo_crates(data_dir: &Path) -> Result<Vec<String>> {
    let index_dir = data_dir.join("cargo/index");
    if !index_dir.exists() {
        debug!("Cargo index directory does not exist");
        return Ok(Vec::new());
    }

    let mut packages = HashSet::new();
    collect_cargo_crates(&index_dir, &mut packages)?;
    let mut packages = packages.into_iter().collect::<Vec<_>>();
    packages.sort();
    Ok(packages)
}

fn collect_cargo_crates(directory: &Path, packages: &mut HashSet<String>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_cargo_crates(&path, packages)?;
        } else if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            if !name.starts_with('.') && name != "config.json" {
                packages.insert(name.to_string());
            }
        }
    }
    Ok(())
}

fn extract_pypi_package_name(filename: &str) -> Option<String> {
    if filename.ends_with(".whl") {
        return filename
            .split_once('-')
            .map(|(name, _)| name.replace('_', "-"));
    }

    let stem = filename.strip_suffix(".tar.gz")?;
    let (name, version) = stem.rsplit_once('-')?;
    version
        .starts_with(|character: char| character.is_numeric() || character == 'v')
        .then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::{extract_pypi_package_name, list_local_packages};

    #[test]
    fn extracts_supported_python_artifact_names() {
        assert_eq!(
            extract_pypi_package_name("shared_auth-1.2.3-py3-none-any.whl").as_deref(),
            Some("shared-auth")
        );
        assert_eq!(
            extract_pypi_package_name("shared-auth-1.2.3.tar.gz").as_deref(),
            Some("shared-auth")
        );
    }

    #[test]
    fn empty_storage_has_a_stable_inventory_shape() {
        let directory = tempfile::tempdir().unwrap();
        let inventory = list_local_packages(directory.path()).unwrap();
        assert_eq!(inventory.len(), 3);
        assert!(inventory.values().all(Vec::is_empty));
    }
}
