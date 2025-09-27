//! Package type detection and information extraction
//!
//! This module handles detection of package types in the current directory
//! and extraction of package names from various project files.

use crate::api::PackageServerClient;
use anyhow::{Context, Result};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

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
            PackageType::Python => super::add_python_package_local(package_name),
            PackageType::Npm => super::add_npm_package_local(package_name),
            PackageType::Cargo => super::add_cargo_package_local(package_name),
        }
    }

    /// Add package to server
    pub fn add_package(&self, client: &PackageServerClient, package_name: &str) -> Result<()> {
        match self {
            PackageType::Python => super::add_python_package(client, package_name),
            PackageType::Npm => super::add_npm_package(client, package_name),
            PackageType::Cargo => super::add_cargo_package(client, package_name),
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
                crate::local_storage::remove_pypi_package_local(package_name, data_dir)
            }
            PackageType::Npm => {
                crate::local_storage::remove_npm_package_local(package_name, data_dir)
            }
            PackageType::Cargo => {
                crate::local_storage::remove_cargo_package_local(package_name, data_dir)
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
