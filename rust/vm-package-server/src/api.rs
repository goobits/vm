use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::Value;
use tracing::{debug, error, info};

/// API client for interacting with the package server
#[allow(dead_code)]
pub struct PackageServerClient {
    client: Client,
    base_url: String,
    auth_token: Option<String>,
}

impl PackageServerClient {
    /// Helper to handle common HTTP response patterns for delete operations
    fn handle_delete_response(
        &self,
        response: reqwest::blocking::Response,
        operation_name: &str,
        item_name: &str,
        success_message: &str,
    ) -> Result<()> {
        if response.status().is_success() {
            info!(item_name = %item_name, "✅ {}: {}", success_message, item_name);
            Ok(())
        } else {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(item_name = %item_name, error = %error_text, "{} failed", operation_name);
            anyhow::bail!("{} failed: {}", operation_name, error_text);
        }
    }

    /// Helper to handle version-specific delete operations
    fn handle_version_delete_response(
        &self,
        response: reqwest::blocking::Response,
        operation_name: &str,
        package_name: &str,
        version: &str,
        success_message: &str,
    ) -> Result<()> {
        if response.status().is_success() {
            info!(package_name = %package_name, version = %version, "✅ {} '{}' version {}", success_message, package_name, version);
            Ok(())
        } else {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(package_name = %package_name, version = %version, error = %error_text, "{} failed", operation_name);
            anyhow::bail!("{} failed: {}", operation_name, error_text);
        }
    }

    /// Helper for Cargo-specific version operations (handles force vs yank)
    fn handle_cargo_version_response(
        &self,
        response: reqwest::blocking::Response,
        crate_name: &str,
        version: &str,
        force: bool,
    ) -> Result<()> {
        if response.status().is_success() {
            if force {
                info!(crate_name = %crate_name, version = %version, "✅ Force deleted Cargo crate '{}' version {}", crate_name, version);
            } else {
                info!(crate_name = %crate_name, version = %version, "✅ Yanked Cargo crate '{}' version {}", crate_name, version);
            }
            Ok(())
        } else {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(crate_name = %crate_name, version = %version, force = %force, error = %error_text, "Cargo crate version operation failed");
            anyhow::bail!("Operation failed: {}", error_text);
        }
    }
}

#[allow(dead_code)]
impl PackageServerClient {
    pub fn new(base_url: &str) -> Self {
        // Check for auth token from environment variable
        let auth_token = std::env::var("PKG_SERVER_AUTH_TOKEN").ok();

        if auth_token.is_some() {
            debug!("Using authentication token from PKG_SERVER_AUTH_TOKEN environment variable");
        }

        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            auth_token,
        }
    }

    /// Check if the server is running
    pub fn is_server_running(&self) -> bool {
        self.client
            .get(format!("{}/", self.base_url))
            .send()
            .map(|resp| resp.status().is_success())
            .unwrap_or(false)
    }

    /// Get all packages from the server
    pub fn get_all_packages(&self) -> Result<serde_json::Value> {
        let response = self
            .client
            .get(format!("{}/api/packages", self.base_url))
            .send()
            .context("Failed to get package list")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to get package list from server");
        }

        response.json().context("Failed to parse package list")
    }

    /// Upload a Python package (wheel or source distribution)
    pub fn upload_pypi_package<P: AsRef<Path>>(&self, file_path: P) -> Result<()> {
        let file_path = file_path.as_ref();
        let file_name = file_path
            .file_name()
            .context("Invalid file path")?
            .to_string_lossy();

        info!(file_path = %file_path.display(), "Uploading PyPI package");

        let _file = File::open(file_path)
            .with_context(|| format!("Failed to open file: {}", file_path.display()))?;

        let form = reqwest::blocking::multipart::Form::new()
            .file("content", file_path)
            .with_context(|| "Failed to create multipart form")?;

        let mut request = self
            .client
            .post(format!("{}/pypi/", self.base_url))
            .multipart(form);

        // Add auth header if token is available
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().context("Failed to upload package")?;

        if response.status().is_success() {
            info!(file_name = %file_name, "✅ Successfully uploaded {}", file_name);
            Ok(())
        } else {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(file_name = %file_name, error = %error_text, "PyPI package upload failed");
            anyhow::bail!("Upload failed: {}", error_text);
        }
    }

    /// Upload an NPM package
    pub fn upload_npm_package(
        &self,
        package_name: &str,
        _tarball_data: &[u8],
        metadata: Value,
    ) -> Result<()> {
        info!(package_name = %package_name, "Uploading NPM package");
        let mut request = self
            .client
            .put(format!("{}/npm/{}", self.base_url, package_name))
            .json(&metadata);

        // Add auth header if token is available
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().context("Failed to upload NPM package")?;

        if response.status().is_success() {
            info!(package_name = %package_name, "✅ Successfully published NPM package: {}", package_name);
            Ok(())
        } else {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(package_name = %package_name, error = %error_text, "NPM package publish failed");
            anyhow::bail!("NPM publish failed: {}", error_text);
        }
    }

    /// Upload a Cargo crate
    pub fn upload_cargo_crate<P: AsRef<Path>>(&self, crate_file: P) -> Result<()> {
        let crate_file = crate_file.as_ref();
        let file_name = crate_file
            .file_name()
            .context("Invalid crate file path")?
            .to_string_lossy();

        info!(crate_file = %crate_file.display(), "Uploading Cargo crate");

        let crate_data = std::fs::read(crate_file)
            .with_context(|| format!("Failed to read crate file: {}", crate_file.display()))?;

        // Extract proper name and version from filename
        let (crate_name, crate_version) = crate::utils::extract_cargo_name_and_version(&file_name)
            .unwrap_or_else(|| ("unknown".to_string(), "1.0.0".to_string()));

        // Create minimal metadata for upload
        let metadata = serde_json::json!({
            "name": crate_name,
            "vers": crate_version,
            "deps": [],
            "features": {},
            "authors": ["test@example.com"],
            "description": "Package uploaded via pkg-server add",
            "homepage": null,
            "documentation": null,
            "readme": null,
            "keywords": [],
            "categories": [],
            "license": "MIT",
            "license_file": null,
            "repository": null,
            "links": null
        });

        let metadata_str = serde_json::to_string(&metadata)?;
        let metadata_bytes = metadata_str.as_bytes();

        // Create cargo publish format: 4-byte metadata length + JSON metadata + 4-byte crate length + .crate file
        let mut payload = Vec::new();

        // 4-byte metadata length (little-endian)
        payload.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());

        // JSON metadata
        payload.extend_from_slice(metadata_bytes);

        // 4-byte crate length (little-endian)
        payload.extend_from_slice(&(crate_data.len() as u32).to_le_bytes());

        // .crate file data
        payload.extend_from_slice(&crate_data);

        let mut request = self
            .client
            .put(format!("{}/cargo/api/v1/crates/new", self.base_url))
            .header("Content-Type", "application/octet-stream");

        // Add auth header if token is available
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .body(payload)
            .send()
            .context("Failed to upload crate")?;

        if response.status().is_success() {
            info!(file_name = %file_name, "✅ Successfully published Cargo crate: {}", file_name);
            Ok(())
        } else {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(file_name = %file_name, error = %error_text, "Cargo crate publish failed");
            anyhow::bail!("Cargo publish failed: {}", error_text);
        }
    }

    /// Delete a Python package
    pub fn delete_pypi_package(&self, package_name: &str) -> Result<()> {
        info!(package_name = %package_name, "Deleting PyPI package");
        let response = self
            .client
            .delete(format!("{}/pypi/package/{}", self.base_url, package_name))
            .send()
            .context("Failed to delete PyPI package")?;

        self.handle_delete_response(
            response,
            "PyPI package deletion",
            package_name,
            "Deleted PyPI package",
        )
    }

    /// Delete an NPM package
    pub fn delete_npm_package(&self, package_name: &str) -> Result<()> {
        info!(package_name = %package_name, "Deleting NPM package");
        let response = self
            .client
            .delete(format!("{}/npm/package/{}", self.base_url, package_name))
            .send()
            .context("Failed to delete NPM package")?;

        self.handle_delete_response(
            response,
            "NPM package deletion",
            package_name,
            "Deleted NPM package",
        )
    }

    /// Delete a Cargo crate
    pub fn delete_cargo_crate(&self, crate_name: &str) -> Result<()> {
        info!(crate_name = %crate_name, "Deleting Cargo crate");
        let response = self
            .client
            .delete(format!("{}/api/cargo/crate/{}", self.base_url, crate_name))
            .send()
            .context("Failed to delete Cargo crate")?;

        self.handle_delete_response(
            response,
            "Cargo crate deletion",
            crate_name,
            "Deleted Cargo crate",
        )
    }

    /// Delete a specific PyPI package version
    pub fn delete_pypi_version(&self, package_name: &str, version: &str) -> Result<()> {
        info!(package_name = %package_name, version = %version, "Deleting PyPI package version");
        let response = self
            .client
            .delete(format!(
                "{}/pypi/{}/{}",
                self.base_url, package_name, version
            ))
            .send()
            .context("Failed to delete PyPI package version")?;

        self.handle_version_delete_response(
            response,
            "PyPI package version deletion",
            package_name,
            version,
            "Deleted PyPI package",
        )
    }

    /// Delete a specific NPM package version
    pub fn delete_npm_version(&self, package_name: &str, version: &str) -> Result<()> {
        info!(package_name = %package_name, version = %version, "Unpublishing NPM package version");
        let response = self
            .client
            .delete(format!(
                "{}/npm/{}/{}",
                self.base_url, package_name, version
            ))
            .send()
            .context("Failed to unpublish NPM package version")?;

        self.handle_version_delete_response(
            response,
            "NPM package version unpublish",
            package_name,
            version,
            "Unpublished NPM package",
        )
    }

    /// Delete or yank a specific Cargo crate version
    pub fn delete_cargo_version(&self, crate_name: &str, version: &str, force: bool) -> Result<()> {
        info!(crate_name = %crate_name, version = %version, force = %force, "Processing Cargo crate version deletion/yank");
        let url = format!(
            "{}/api/cargo/{}/{}?force={}",
            self.base_url, crate_name, version, force
        );
        let response = self
            .client
            .delete(url)
            .send()
            .context("Failed to process Cargo crate version")?;

        self.handle_cargo_version_response(response, crate_name, version, force)
    }

    /// Get available versions for a PyPI package
    pub fn get_pypi_versions(&self, package_name: &str) -> Result<Vec<String>> {
        debug!(package_name = %package_name, "Fetching PyPI package versions");
        let response = self
            .client
            .get(format!(
                "{}/api/packages/pypi/{}/versions",
                self.base_url, package_name
            ))
            .send()
            .context("Failed to fetch PyPI package versions")?;

        if response.status().is_success() {
            let versions: Vec<String> = response
                .json()
                .context("Failed to parse versions response")?;
            Ok(versions)
        } else {
            // If endpoint doesn't exist, return empty list
            Ok(Vec::new())
        }
    }

    /// Get available versions for an NPM package
    pub fn get_npm_versions(&self, package_name: &str) -> Result<Vec<String>> {
        debug!(package_name = %package_name, "Fetching NPM package versions");
        let response = self
            .client
            .get(format!("{}/npm/{}", self.base_url, package_name))
            .send()
            .context("Failed to fetch NPM package metadata")?;

        if response.status().is_success() {
            let metadata: Value = response.json().context("Failed to parse NPM metadata")?;

            if let Some(versions_obj) = metadata.get("versions").and_then(|v| v.as_object()) {
                let versions: Vec<String> = versions_obj.keys().cloned().collect();
                Ok(versions)
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Get available versions for a Cargo crate
    pub fn get_cargo_versions(&self, crate_name: &str) -> Result<Vec<String>> {
        debug!(crate_name = %crate_name, "Fetching Cargo crate versions");
        let response = self
            .client
            .get(format!(
                "{}/api/v1/cargo/{}/versions",
                self.base_url, crate_name
            ))
            .send()
            .context("Failed to fetch Cargo crate versions")?;

        if response.status().is_success() {
            let versions: Vec<String> = response
                .json()
                .context("Failed to parse Cargo versions response")?;
            Ok(versions)
        } else {
            // If endpoint doesn't exist or crate not found, return empty list
            Ok(Vec::new())
        }
    }

    /// List all packages across all ecosystems
    pub fn list_all_packages(&self) -> Result<HashMap<String, Vec<String>>> {
        debug!("Fetching all packages from server");
        let response = self
            .client
            .get(format!("{}/api/packages", self.base_url))
            .send()
            .context("Failed to fetch package list")?;

        if response.status().is_success() {
            let packages: HashMap<String, Vec<String>> = response
                .json()
                .context("Failed to parse package list response")?;
            Ok(packages)
        } else {
            anyhow::bail!("Failed to fetch packages");
        }
    }

    /// Get server status and stats
    pub fn get_server_status(&self) -> Result<Value> {
        debug!("Fetching server status");
        let response = self
            .client
            .get(format!("{}/api/status", self.base_url))
            .send()
            .context("Failed to fetch server status")?;

        if response.status().is_success() {
            let status: Value = response.json().context("Failed to parse status response")?;
            Ok(status)
        } else {
            anyhow::bail!("Failed to fetch server status");
        }
    }
}