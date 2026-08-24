use crate::response_body::read_bounded_response;
use crate::{AppError, AppResult};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, info, warn};
use url::Url;

/// Configuration for upstream package registries.
///
/// This struct defines the connection settings and URLs for communicating with
/// upstream package registries (PyPI, npm, crates.io). It controls how the server
/// fetches packages and metadata from external sources when they're not available locally.
///
/// # Fields
///
/// * `pypi_url` - Base URL for the PyPI registry (default: <https://pypi.org>)
/// * `npm_url` - Base URL for the npm registry (default: <https://registry.npmjs.org>)
/// * `cargo_url` - Base URL for the Cargo registry (default: <https://index.crates.io>)
/// * `timeout` - HTTP request timeout for upstream calls
/// * `enabled` - Whether upstream registry lookups are enabled
///
/// # Examples
///
/// ```rust
/// use std::time::Duration;
/// use vm_package_server::UpstreamConfig;
///
/// // Use default configuration
/// let config = UpstreamConfig::default();
///
/// // Custom configuration
/// let config = UpstreamConfig {
///     pypi_url: "https://pypi.org".to_string(),
///     npm_url: "https://registry.npmjs.org".to_string(),
///     cargo_url: "https://index.crates.io".to_string(),
///     timeout: Duration::from_secs(30),
///     enabled: true,
/// };
/// ```
#[derive(Clone)]
pub struct UpstreamConfig {
    /// Base URL for PyPI registry API
    pub pypi_url: String,
    /// Base URL for npm registry API
    pub npm_url: String,
    /// Base URL for Cargo registry index
    pub cargo_url: String,
    /// HTTP request timeout for upstream calls
    pub timeout: Duration,
    /// Whether upstream registry lookups are enabled
    pub enabled: bool,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            pypi_url: "https://pypi.org".to_string(),
            npm_url: "https://registry.npmjs.org".to_string(),
            cargo_url: "https://index.crates.io".to_string(),
            timeout: Duration::from_secs(30),
            enabled: true,
        }
    }
}

/// HTTP client for upstream registry communication.
///
/// This struct provides methods to fetch packages, metadata, and other resources
/// from upstream registries (PyPI, npm, crates.io). It handles HTTP requests,
/// error handling, and response processing for different registry types.
///
/// # Features
///
/// - **Timeout handling**: Configurable request timeouts
/// - **Error handling**: Converts HTTP errors to application errors
/// - **Registry-specific APIs**: Dedicated methods for each registry type
/// - **Response processing**: Handles different response formats (HTML, JSON, binary)
/// - **URL rewriting**: Updates npm tarball URLs to point to local server
///
/// # Examples
///
/// ```rust,no_run
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use vm_package_server::{UpstreamClient, UpstreamConfig};
///
/// let config = UpstreamConfig::default();
/// let client = UpstreamClient::new(config)?;
///
/// // Fetch PyPI package index
/// let html = client.fetch_pypi_simple("requests").await?;
///
/// // Fetch npm package metadata
/// let metadata = client.fetch_npm_metadata("express").await?;
///
/// // Stream a package file
/// let data = client.stream_pypi_file("source/r/requests/requests-2.31.0.tar.gz").await?;
/// # Ok(())
/// # }
/// ```
pub struct UpstreamClient {
    client: Option<Client>,
    config: UpstreamConfig,
}

impl UpstreamClient {
    /// Create a new upstream client with the given configuration.
    ///
    /// Initializes an HTTP client with the specified timeout and user agent
    /// for communicating with upstream registries.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for upstream registry URLs and settings
    ///
    /// # Returns
    ///
    /// * `Ok(UpstreamClient)` if the HTTP client was successfully created
    /// * `Err(AppError)` if the HTTP client initialization failed
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be created.
    pub fn new(config: UpstreamConfig) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent("goobits-pkg-server/0.1.0")
            .build()
            .map_err(|e| AppError::InternalError(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self {
            client: Some(client),
            config,
        })
    }

    /// Create a disabled upstream client for testing.
    ///
    /// This constructor creates a client with `enabled = false` and NO HTTP client.
    /// This completely avoids TLS initialization and macOS Keychain prompts.
    /// Since `enabled = false`, all methods will return errors before trying to use the client.
    ///
    /// # Returns
    ///
    /// A disabled UpstreamClient that will return errors for all requests
    #[cfg(test)]
    pub fn disabled() -> Self {
        let config = UpstreamConfig {
            enabled: false,
            ..Default::default()
        };

        // No client creation = no TLS initialization = no Keychain prompt
        Self {
            client: None,
            config,
        }
    }

    /// Get the HTTP client, returning an error if not available
    fn get_client(&self) -> AppResult<&Client> {
        self.client.as_ref().ok_or_else(|| {
            AppError::InternalError(
                "HTTP client not initialized (upstream is disabled)".to_string(),
            )
        })
    }

    /// Fetch PyPI simple index HTML for a package.
    ///
    /// Retrieves the simple API index page for a PyPI package, which contains
    /// links to all available versions and their download URLs. This follows
    /// the PyPI simple API specification (PEP 503).
    ///
    /// # Arguments
    ///
    /// * `package_name` - The name of the PyPI package to fetch
    ///
    /// # Returns
    ///
    /// * `Ok(String)` containing the HTML index page
    /// * `Err(AppError::NotFound)` if the package doesn't exist or upstream is disabled
    /// * `Err(AppError::InternalError)` if the request failed or response parsing failed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use vm_package_server::{UpstreamClient, UpstreamConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = UpstreamClient::new(UpstreamConfig::default())?;
    /// let html = client.fetch_pypi_simple("requests").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn fetch_pypi_simple(&self, package_name: &str) -> AppResult<String> {
        if !self.config.enabled {
            return Err(AppError::NotFound(
                "Upstream registry lookup is disabled in configuration".to_string(),
            ));
        }

        let url = format!("{}/simple/{}/", self.config.pypi_url, package_name);
        debug!(url = %url, "Fetching PyPI simple index");

        let response = self
            .get_client()?
            .get(&url)
            .header("Accept", "text/html,application/vnd.pypi.simple.v1+html")
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to fetch from PyPI");
                AppError::NotFound(format!("Package not found on PyPI: {package_name}"))
            })?;

        if response.status().is_success() {
            let content = response.text().await.map_err(|e| {
                AppError::InternalError(format!("Failed to read PyPI response: {e}"))
            })?;
            info!(package = %package_name, "Successfully fetched from PyPI");
            Ok(content)
        } else {
            Err(AppError::NotFound(format!(
                "Package not found on PyPI: {package_name}"
            )))
        }
    }

    /// Stream a file from PyPI with proper streaming and size validation
    pub async fn stream_pypi_file(&self, filename: &str) -> AppResult<bytes::Bytes> {
        if !self.config.enabled {
            return Err(AppError::NotFound(
                "Upstream registry lookup is disabled in configuration".to_string(),
            ));
        }

        let url = format!("https://files.pythonhosted.org/packages/{filename}");
        debug!(url = %url, "Streaming file from PyPI");

        let response = self.get_client()?.get(&url).send().await.map_err(|e| {
            warn!(error = %e, "Failed to fetch file from PyPI");
            AppError::NotFound(format!("File not found on PyPI: {filename}"))
        })?;

        if !response.status().is_success() {
            return Err(AppError::NotFound(format!(
                "File not found on PyPI: {filename}"
            )));
        }

        // Use centralized validation and streaming logic
        read_bounded_response(response, "PyPI", filename).await
    }

    /// Fetch npm package metadata as JSON.
    ///
    /// Retrieves the complete metadata for an npm package, including all versions,
    /// dependencies, and download URLs. This returns the full registry document
    /// for the package.
    ///
    /// # Arguments
    ///
    /// * `package_name` - The name of the npm package to fetch
    ///
    /// # Returns
    ///
    /// * `Ok(Value)` containing the parsed JSON metadata
    /// * `Err(AppError::NotFound)` if the package doesn't exist or upstream is disabled
    /// * `Err(AppError::InternalError)` if the request failed or JSON parsing failed
    pub async fn fetch_npm_metadata(&self, package_name: &str) -> AppResult<Value> {
        if !self.config.enabled {
            return Err(AppError::NotFound(
                "Upstream registry lookup is disabled in configuration".to_string(),
            ));
        }

        let url = format!("{}/{}", self.config.npm_url, package_name);
        debug!(url = %url, "Fetching NPM metadata");

        let response = self
            .get_client()?
            .get(&url)
            .header("Accept", "application/vnd.npm.install-v1+json")
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to fetch from NPM");
                AppError::NotFound(format!("Package not found on NPM: {package_name}"))
            })?;

        if response.status().is_success() {
            let metadata = response.json().await.map_err(|e| {
                AppError::InternalError(format!("Failed to parse NPM response: {e}"))
            })?;
            info!(package = %package_name, "Successfully fetched from NPM");
            Ok(metadata)
        } else {
            Err(AppError::NotFound(format!(
                "Package not found on NPM: {package_name}"
            )))
        }
    }

    /// Stream an NPM tarball with proper streaming and size validation
    pub async fn stream_npm_tarball(&self, tarball_url: &str) -> AppResult<bytes::Bytes> {
        if !self.config.enabled {
            return Err(AppError::NotFound(
                "Upstream registry lookup is disabled in configuration".to_string(),
            ));
        }

        // Handle both absolute and relative URLs
        let full_url = if tarball_url.starts_with("http") {
            tarball_url.to_string()
        } else {
            format!("{}{}", self.config.npm_url, tarball_url)
        };

        debug!(url = %full_url, "Streaming tarball from NPM");

        let response = self
            .get_client()?
            .get(&full_url)
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to fetch tarball from NPM");
                AppError::NotFound("Tarball not found on NPM".to_string())
            })?;

        if !response.status().is_success() {
            return Err(AppError::NotFound("Tarball not found on NPM".to_string()));
        }

        // Use centralized validation and streaming logic
        read_bounded_response(response, "NPM", &full_url).await
    }

    /// Fetch Cargo crate index
    pub async fn fetch_cargo_index(&self, crate_name: &str, index_path: &str) -> AppResult<String> {
        if !self.config.enabled {
            return Err(AppError::NotFound(
                "Upstream registry lookup is disabled in configuration".to_string(),
            ));
        }

        let url = format!("{}/{}", self.config.cargo_url, index_path);
        debug!(url = %url, "Fetching Cargo index");

        let response = self.get_client()?.get(&url).send().await.map_err(|e| {
            warn!(error = %e, "Failed to fetch from Cargo index");
            AppError::NotFound(format!("Crate not found on crates.io: {crate_name}"))
        })?;

        if response.status().is_success() {
            let content = response.text().await.map_err(|e| {
                AppError::InternalError(format!("Failed to read Cargo response: {e}"))
            })?;
            info!(crate_name = %crate_name, "Successfully fetched from crates.io");
            Ok(content)
        } else {
            Err(AppError::NotFound(format!(
                "Crate not found on crates.io: {crate_name}"
            )))
        }
    }

    /// Stream a Cargo crate file with proper streaming and size validation
    pub async fn stream_cargo_crate(
        &self,
        crate_name: &str,
        version: &str,
    ) -> AppResult<bytes::Bytes> {
        if !self.config.enabled {
            return Err(AppError::NotFound(
                "Upstream registry lookup is disabled in configuration".to_string(),
            ));
        }

        // Construct download URL from crates.io
        let url = format!("https://crates.io/api/v1/crates/{crate_name}/{version}/download");
        debug!(url = %url, "Streaming crate from crates.io");

        let response = self.get_client()?.get(&url).send().await.map_err(|e| {
            warn!(error = %e, "Failed to fetch crate from crates.io");
            AppError::NotFound(format!("Crate not found: {crate_name}-{version}"))
        })?;

        if !response.status().is_success() {
            return Err(AppError::NotFound(format!(
                "Crate not found: {crate_name}-{version}"
            )));
        }

        // Use centralized validation and streaming logic
        let crate_filename = format!("{crate_name}-{version}.crate");
        read_bounded_response(response, "Cargo", &crate_filename).await
    }

    /// Update tarball URLs in npm metadata to point to the current server.
    ///
    /// Modifies npm package metadata to replace upstream tarball URLs with URLs
    /// pointing to the current server. This allows npm clients to download
    /// packages from the local cache instead of the upstream registry.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The npm package metadata JSON to modify
    /// * `server_addr` - The server address to use in the new URLs
    ///
    /// # Returns
    ///
    /// The modified metadata with updated tarball URLs
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use vm_package_server::{UpstreamClient, UpstreamConfig};
    /// # use serde_json::Value;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = UpstreamClient::new(UpstreamConfig::default())?;
    /// let mut metadata = client.fetch_npm_metadata("express").await?;
    /// metadata = client.update_npm_tarball_urls(
    ///     metadata,
    ///     "http://localhost:3080",
    ///     "express",
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn update_npm_tarball_urls(
        &self,
        mut metadata: Value,
        server_addr: &str,
        package: &str,
    ) -> Value {
        if let Some(versions) = metadata["versions"].as_object_mut() {
            for version_data in versions.values_mut() {
                if let Some(dist) = version_data.get_mut("dist").and_then(|d| d.as_object_mut()) {
                    if let Some(tarball_url) = dist.get("tarball").and_then(|t| t.as_str()) {
                        // Extract filename from original URL
                        if let Ok(url) = Url::parse(tarball_url) {
                            if let Some(filename) = url
                                .path_segments()
                                .and_then(|mut segments| segments.next_back())
                            {
                                if let Some(new_tarball_url) =
                                    npm_proxy_tarball_url(server_addr, package, filename)
                                {
                                    dist.insert(
                                        "tarball".to_string(),
                                        Value::String(new_tarball_url),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        metadata
    }
}

fn npm_proxy_tarball_url(server_addr: &str, package: &str, filename: &str) -> Option<String> {
    let mut url = Url::parse(server_addr).ok()?;
    let mut segments = url.path_segments_mut().ok()?;
    segments.pop_if_empty();
    segments.extend(["npm", package, "-", filename]);
    drop(segments);
    Some(url.into())
}

#[cfg(test)]
mod tests {
    use super::UpstreamClient;
    use serde_json::json;

    #[test]
    fn npm_metadata_points_tarballs_at_the_package_route() {
        let metadata = json!({
            "versions": {
                "7.0.0": {
                    "dist": {
                        "tarball": "https://registry.npmjs.org/is-number/-/is-number-7.0.0.tgz"
                    }
                }
            }
        });

        let updated = UpstreamClient::disabled().update_npm_tarball_urls(
            metadata,
            "http://packages.internal:3080",
            "is-number",
        );

        assert_eq!(
            updated["versions"]["7.0.0"]["dist"]["tarball"],
            "http://packages.internal:3080/npm/is-number/-/is-number-7.0.0.tgz"
        );
    }

    #[test]
    fn npm_metadata_encodes_scoped_packages_as_one_route_segment() {
        let metadata = json!({
            "versions": {
                "1.0.0": {
                    "dist": {
                        "tarball": "https://registry.npmjs.org/@goobits/auth/-/auth-1.0.0.tgz"
                    }
                }
            }
        });

        let updated = UpstreamClient::disabled().update_npm_tarball_urls(
            metadata,
            "http://packages.internal:3080",
            "@goobits/auth",
        );

        assert_eq!(
            updated["versions"]["1.0.0"]["dist"]["tarball"],
            "http://packages.internal:3080/npm/@goobits%2Fauth/-/auth-1.0.0.tgz"
        );
    }
}
