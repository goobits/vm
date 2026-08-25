use std::time::Duration;

use reqwest::Client;

use crate::{AppError, AppResult};

mod cargo;
mod npm;
mod pypi;

#[derive(Clone)]
pub struct UpstreamConfig {
    pub pypi_url: String,
    pub npm_url: String,
    pub cargo_url: String,
    pub timeout: Duration,
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

pub struct UpstreamClient {
    client: Option<Client>,
    config: UpstreamConfig,
}

impl UpstreamClient {
    pub fn new(config: UpstreamConfig) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent("goobits-pkg-server/0.1.0")
            .build()
            .map_err(|error| {
                AppError::InternalError(format!("Failed to create HTTP client: {error}"))
            })?;
        Ok(Self {
            client: Some(client),
            config,
        })
    }

    #[cfg(test)]
    pub fn disabled() -> Self {
        Self {
            client: None,
            config: UpstreamConfig {
                enabled: false,
                ..Default::default()
            },
        }
    }

    fn client(&self) -> AppResult<&Client> {
        self.client.as_ref().ok_or_else(|| {
            AppError::InternalError(
                "HTTP client not initialized (upstream is disabled)".to_string(),
            )
        })
    }

    fn require_enabled(&self) -> AppResult<()> {
        if self.config.enabled {
            Ok(())
        } else {
            Err(AppError::NotFound(
                "Upstream registry lookup is disabled in configuration".to_string(),
            ))
        }
    }
}
