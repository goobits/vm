use tracing::{debug, warn};

use super::UpstreamClient;
use crate::response_body::read_bounded_response;
use crate::{AppError, AppResult};

impl UpstreamClient {
    pub async fn fetch_pypi_simple(&self, package_name: &str) -> AppResult<String> {
        self.require_enabled()?;
        let url = format!("{}/simple/{}/", self.config.pypi_url, package_name);
        let response = self
            .client()?
            .get(&url)
            .header("Accept", "text/html,application/vnd.pypi.simple.v1+html")
            .send()
            .await
            .map_err(|error| {
                warn!(
                    operation = "fetch_metadata",
                    ecosystem = "python",
                    package = %package_name,
                    error = ?error.without_url(),
                    "upstream package request failed"
                );
                AppError::NotFound(format!("Package not found on PyPI: {package_name}"))
            })?;
        if !response.status().is_success() {
            return Err(AppError::NotFound(format!(
                "Package not found on PyPI: {package_name}"
            )));
        }
        let content = response.text().await.map_err(|error| {
            AppError::InternalError(format!(
                "Failed to read PyPI response: {}",
                error.without_url()
            ))
        })?;
        debug!(
            operation = "fetch_metadata",
            ecosystem = "python",
            package = %package_name,
            "upstream package metadata fetched"
        );
        Ok(content)
    }

    pub async fn stream_pypi_file(&self, filename: &str) -> AppResult<bytes::Bytes> {
        self.require_enabled()?;
        let url = format!("https://files.pythonhosted.org/packages/{filename}");
        let response = self.client()?.get(&url).send().await.map_err(|error| {
            warn!(
                operation = "download",
                ecosystem = "python",
                error = ?error.without_url(),
                "upstream package request failed"
            );
            AppError::NotFound(format!("File not found on PyPI: {filename}"))
        })?;
        if !response.status().is_success() {
            return Err(AppError::NotFound(format!(
                "File not found on PyPI: {filename}"
            )));
        }
        read_bounded_response(response, "PyPI", filename).await
    }
}
