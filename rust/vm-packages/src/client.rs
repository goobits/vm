use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::RegistryEndpoints;

pub type PackageInventory = BTreeMap<String, Vec<String>>;

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
}

impl PackageInfrastructureClient {
    pub fn new(endpoints: RegistryEndpoints) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoints,
        }
    }

    pub async fn is_healthy(&self) -> bool {
        self.http
            .get(format!("{}/health", self.endpoints.gateway()))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    pub async fn status(&self) -> Result<InfrastructureStatus> {
        self.get_json("api/status").await
    }

    pub async fn packages(&self) -> Result<PackageInventory> {
        self.get_json("api/packages").await
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}/{}", self.endpoints.gateway(), path);
        self.http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("failed to connect to package infrastructure at {url}"))?
            .error_for_status()
            .with_context(|| format!("package infrastructure rejected GET {url}"))?
            .json()
            .await
            .with_context(|| format!("package infrastructure returned invalid JSON from {url}"))
    }
}
