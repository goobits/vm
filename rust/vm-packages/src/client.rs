mod catalog;
mod checkouts;
mod releases;
mod rollouts;
mod submissions;
mod tools;
mod transport;

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::RegistryEndpoints;

pub type PackageInventory = BTreeMap<String, Vec<String>>;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfrastructureStatus {
    pub status: String,
    pub service: String,
    pub version: String,
    pub registries: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PackageInfrastructureClient {
    http: reqwest::Client,
    endpoints: RegistryEndpoints,
    read_token: Option<String>,
    agent_token: Option<String>,
    controller_token: Option<String>,
    reviewer_token: Option<String>,
    build_token: Option<String>,
    release_token: Option<String>,
    rollout_token: Option<String>,
}

impl PackageInfrastructureClient {
    pub fn new(endpoints: RegistryEndpoints) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("static package client settings are valid"),
            endpoints,
            read_token: None,
            agent_token: None,
            controller_token: None,
            reviewer_token: None,
            build_token: None,
            release_token: None,
            rollout_token: None,
        }
    }

    pub fn with_read_token(mut self, token: impl Into<String>) -> Self {
        self.read_token = Some(token.into());
        self
    }

    pub fn with_agent_token(mut self, token: impl Into<String>) -> Self {
        self.agent_token = Some(token.into());
        self
    }

    pub fn with_controller_token(mut self, token: impl Into<String>) -> Self {
        self.controller_token = Some(token.into());
        self
    }

    pub fn with_reviewer_token(mut self, token: impl Into<String>) -> Self {
        self.reviewer_token = Some(token.into());
        self
    }

    pub fn with_release_token(mut self, token: impl Into<String>) -> Self {
        self.release_token = Some(token.into());
        self
    }

    pub fn with_build_token(mut self, token: impl Into<String>) -> Self {
        self.build_token = Some(token.into());
        self
    }

    pub fn with_rollout_token(mut self, token: impl Into<String>) -> Self {
        self.rollout_token = Some(token.into());
        self
    }

    pub async fn is_healthy(&self) -> bool {
        self.http
            .get(format!("{}/health", self.endpoints.gateway()))
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    pub async fn is_work_healthy(&self) -> bool {
        self.http
            .get(self.work_url("health"))
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    pub async fn is_oci_healthy(&self) -> bool {
        self.http
            .get(self.endpoints.oci())
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    pub async fn is_fully_healthy(&self) -> bool {
        let (gateway, work, oci) = tokio::join!(
            self.is_healthy(),
            self.is_work_healthy(),
            self.is_oci_healthy()
        );
        gateway && work && oci
    }

    pub async fn status(&self) -> Result<InfrastructureStatus> {
        self.get_json("api/status").await
    }
}

#[cfg(test)]
mod tests {
    use super::PackageInfrastructureClient;
    use crate::RegistryEndpoints;

    #[test]
    fn checkout_archive_url_is_gateway_scoped_and_encoded() {
        let client = PackageInfrastructureClient::new(
            RegistryEndpoints::new("https://packages.internal").unwrap(),
        );
        assert_eq!(
            client.checkout_archive_url("checkout-1", "project/a"),
            "https://packages.internal/work/v1/checkouts/checkout-1/archive?consumer=project%2Fa"
        );
        assert_eq!(
            client.review_bundle_url("submission-1"),
            "https://packages.internal/work/v1/submissions/submission-1/review-bundle"
        );
    }
}
