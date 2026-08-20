use std::time::Duration;

use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;
use url::Url;
use vm_packages::{InternalPackageCatalog, RegistryEndpoints};

use crate::response_body::read_bounded_response;
use crate::{AppError, AppResult};

const INTERNAL_METADATA_TIMEOUT: Duration = Duration::from_secs(2);
const INTERNAL_ARTIFACT_TIMEOUT: Duration = Duration::from_secs(120);

/// Authenticated, bounded client used only by worker-edge processes.
#[derive(Debug)]
pub struct InternalRegistryClient {
    http: Client,
    gateway: Url,
    token: String,
}

impl InternalRegistryClient {
    pub fn from_environment() -> AppResult<Option<Self>> {
        let Ok(gateway) = std::env::var("PKG_SERVER_INTERNAL_GATEWAY") else {
            return Ok(None);
        };
        let token = read_token()?;
        Self::new(gateway, token).map(Some)
    }

    pub fn new(gateway: impl Into<String>, token: impl Into<String>) -> AppResult<Self> {
        let endpoints = RegistryEndpoints::new(gateway.into())
            .map_err(|error| AppError::BadRequest(error.to_string()))?;
        let token = token.into();
        if token.trim().is_empty() {
            return Err(AppError::BadRequest(
                "internal package token cannot be empty".into(),
            ));
        }
        let http = Client::builder()
            .connect_timeout(Duration::from_millis(500))
            .user_agent("vm-package-edge")
            .build()
            .map_err(|error| AppError::InternalError(error.to_string()))?;
        Ok(Self {
            http,
            gateway: Url::parse(endpoints.gateway())
                .map_err(|error| AppError::BadRequest(error.to_string()))?,
            token,
        })
    }

    pub fn gateway(&self) -> &str {
        self.gateway.as_str().trim_end_matches('/')
    }

    pub async fn catalog(&self) -> AppResult<InternalPackageCatalog> {
        self.get_json("work/v1/catalog").await
    }

    pub async fn npm_metadata(&self, package: &str) -> AppResult<serde_json::Value> {
        self.get_json(&format!("npm/{}", encode_segment(package)))
            .await
    }

    pub async fn npm_tarball(&self, package: &str, filename: &str) -> AppResult<bytes::Bytes> {
        self.get_bytes(
            &format!("npm/{}/-/{filename}", encode_segment(package)),
            "npm",
            filename,
        )
        .await
    }

    pub async fn pypi_index(&self, package: &str) -> AppResult<String> {
        self.get_text(&format!("pypi/simple/{}/", encode_segment(package)))
            .await
    }

    pub async fn pypi_artifact(&self, path: &str) -> AppResult<bytes::Bytes> {
        self.get_bytes(&format!("pypi/{path}"), "PyPI", path).await
    }

    pub async fn cargo_index(&self, path: &str) -> AppResult<String> {
        self.get_text(&format!("cargo/index/{path}")).await
    }

    pub async fn cargo_crate(&self, name: &str, version: &str) -> AppResult<bytes::Bytes> {
        let filename = format!("{name}-{version}.crate");
        self.get_bytes(
            &format!(
                "cargo/api/v1/crates/{}/{}/download",
                encode_segment(name),
                encode_segment(version)
            ),
            "Cargo",
            &filename,
        )
        .await
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> AppResult<T> {
        self.response(path, INTERNAL_METADATA_TIMEOUT)
            .await?
            .json()
            .await
            .map_err(|error| AppError::Unavailable(error.to_string()))
    }

    async fn get_text(&self, path: &str) -> AppResult<String> {
        self.response(path, INTERNAL_METADATA_TIMEOUT)
            .await?
            .text()
            .await
            .map_err(|error| AppError::Unavailable(error.to_string()))
    }

    async fn get_bytes(&self, path: &str, ecosystem: &str, name: &str) -> AppResult<bytes::Bytes> {
        read_bounded_response(
            self.response(path, INTERNAL_ARTIFACT_TIMEOUT).await?,
            ecosystem,
            name,
        )
        .await
    }

    async fn response(&self, path: &str, timeout: Duration) -> AppResult<Response> {
        let url = format!("{}/{}", self.gateway(), path.trim_start_matches('/'));
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .timeout(timeout)
            .send()
            .await
            .map_err(|error| AppError::Unavailable(error.to_string()))?;
        match response.status() {
            status if status.is_success() => Ok(response),
            StatusCode::NOT_FOUND => Err(AppError::NotFound("internal package not found".into())),
            status => Err(AppError::Unavailable(format!(
                "internal package service returned {status}"
            ))),
        }
    }
}

fn read_token() -> AppResult<String> {
    let token = if let Some(path) = std::env::var_os("PKG_SERVER_INTERNAL_TOKEN_FILE") {
        std::fs::read_to_string(path)?
    } else {
        std::env::var("PKG_SERVER_INTERNAL_TOKEN").map_err(|_| {
            AppError::BadRequest(
                "PKG_SERVER_INTERNAL_TOKEN or PKG_SERVER_INTERNAL_TOKEN_FILE is required".into(),
            )
        })?
    };
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AppError::BadRequest(
            "internal package token cannot be empty".into(),
        ));
    }
    Ok(token)
}

fn encode_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::encode_segment;

    #[test]
    fn native_names_remain_one_url_segment() {
        assert_eq!(encode_segment("@goobits/auth"), "%40goobits%2Fauth");
    }
}
