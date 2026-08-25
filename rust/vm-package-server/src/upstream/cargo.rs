use tracing::{debug, warn};

use super::UpstreamClient;
use crate::response_body::read_bounded_response;
use crate::{AppError, AppResult};

impl UpstreamClient {
    pub async fn fetch_cargo_index(&self, crate_name: &str, index_path: &str) -> AppResult<String> {
        self.require_enabled()?;
        let url = format!("{}/{}", self.config.cargo_url, index_path);
        let response = self.client()?.get(&url).send().await.map_err(|error| {
            warn!(
                operation = "fetch_metadata",
                ecosystem = "cargo",
                package = %crate_name,
                error = ?error.without_url(),
                "upstream package request failed"
            );
            AppError::NotFound(format!("Crate not found on crates.io: {crate_name}"))
        })?;
        if !response.status().is_success() {
            return Err(AppError::NotFound(format!(
                "Crate not found on crates.io: {crate_name}"
            )));
        }
        let content = response.text().await.map_err(|error| {
            AppError::InternalError(format!(
                "Failed to read Cargo response: {}",
                error.without_url()
            ))
        })?;
        debug!(
            operation = "fetch_metadata",
            ecosystem = "cargo",
            package = %crate_name,
            "upstream package metadata fetched"
        );
        Ok(content)
    }

    pub async fn stream_cargo_crate(
        &self,
        crate_name: &str,
        version: &str,
    ) -> AppResult<bytes::Bytes> {
        self.require_enabled()?;
        let url = format!("https://crates.io/api/v1/crates/{crate_name}/{version}/download");
        let response = self.client()?.get(&url).send().await.map_err(|error| {
            warn!(
                operation = "download",
                ecosystem = "cargo",
                package = %crate_name,
                version = %version,
                error = ?error.without_url(),
                "upstream package request failed"
            );
            AppError::NotFound(format!("Crate not found: {crate_name}-{version}"))
        })?;
        if !response.status().is_success() {
            return Err(AppError::NotFound(format!(
                "Crate not found: {crate_name}-{version}"
            )));
        }
        read_bounded_response(response, "Cargo", &format!("{crate_name}-{version}.crate")).await
    }
}
