use serde_json::Value;
use tracing::{debug, warn};
use url::Url;

use super::UpstreamClient;
use crate::response_body::read_bounded_response;
use crate::{AppError, AppResult};

impl UpstreamClient {
    pub async fn fetch_npm_metadata(&self, package_name: &str) -> AppResult<Value> {
        self.require_enabled()?;
        let url = format!("{}/{}", self.config.npm_url, package_name);
        let response = self
            .client()?
            .get(&url)
            .header("Accept", "application/vnd.npm.install-v1+json")
            .send()
            .await
            .map_err(|error| {
                warn!(
                    operation = "fetch_metadata",
                    ecosystem = "npm",
                    package = %package_name,
                    error = ?error.without_url(),
                    "upstream package request failed"
                );
                AppError::NotFound(format!("Package not found on NPM: {package_name}"))
            })?;
        if !response.status().is_success() {
            return Err(AppError::NotFound(format!(
                "Package not found on NPM: {package_name}"
            )));
        }
        let metadata = response.json().await.map_err(|error| {
            AppError::InternalError(format!(
                "Failed to parse NPM response: {}",
                error.without_url()
            ))
        })?;
        debug!(
            operation = "fetch_metadata",
            ecosystem = "npm",
            package = %package_name,
            "upstream package metadata fetched"
        );
        Ok(metadata)
    }

    pub async fn stream_npm_tarball(&self, tarball_url: &str) -> AppResult<bytes::Bytes> {
        self.require_enabled()?;
        let url = if tarball_url.starts_with("http") {
            tarball_url.to_string()
        } else {
            format!("{}{}", self.config.npm_url, tarball_url)
        };
        let response = self.client()?.get(&url).send().await.map_err(|error| {
            warn!(
                operation = "download",
                ecosystem = "npm",
                error = ?error.without_url(),
                "upstream package request failed"
            );
            AppError::NotFound("Tarball not found on NPM".to_string())
        })?;
        if !response.status().is_success() {
            return Err(AppError::NotFound("Tarball not found on NPM".to_string()));
        }
        read_bounded_response(response, "NPM", &url).await
    }

    pub fn update_npm_tarball_urls(
        &self,
        mut metadata: Value,
        server_addr: &str,
        package: &str,
    ) -> Value {
        let Some(versions) = metadata["versions"].as_object_mut() else {
            return metadata;
        };
        for version in versions.values_mut() {
            let Some(dist) = version.get_mut("dist").and_then(Value::as_object_mut) else {
                continue;
            };
            let Some(tarball) = dist.get("tarball").and_then(Value::as_str) else {
                continue;
            };
            let Some(filename) = Url::parse(tarball)
                .ok()
                .and_then(|url| url.path_segments()?.next_back().map(str::to_string))
            else {
                continue;
            };
            if let Some(url) = proxy_tarball_url(server_addr, package, &filename) {
                dist.insert("tarball".to_string(), Value::String(url));
            }
        }
        metadata
    }
}

fn proxy_tarball_url(server_addr: &str, package: &str, filename: &str) -> Option<String> {
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
    fn metadata_points_tarballs_at_the_package_route() {
        let metadata = json!({
            "versions": {"7.0.0": {"dist": {
                "tarball": "https://registry.npmjs.org/is-number/-/is-number-7.0.0.tgz"
            }}}
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
    fn metadata_encodes_scoped_packages_as_one_route_segment() {
        let metadata = json!({
            "versions": {"1.0.0": {"dist": {
                "tarball": "https://registry.npmjs.org/@goobits/auth/-/auth-1.0.0.tgz"
            }}}
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
