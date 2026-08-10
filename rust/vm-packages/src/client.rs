use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    CheckoutLease, CheckoutRecord, CreateCheckout, LeaseRequest, RegistryEndpoints,
    TransitionRequest, WorkflowReceipt,
};

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
    read_token: Option<String>,
    controller_token: Option<String>,
}

impl PackageInfrastructureClient {
    pub fn new(endpoints: RegistryEndpoints) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoints,
            read_token: None,
            controller_token: None,
        }
    }

    pub fn with_read_token(mut self, token: impl Into<String>) -> Self {
        self.read_token = Some(token.into());
        self
    }

    pub fn with_controller_token(mut self, token: impl Into<String>) -> Self {
        self.controller_token = Some(token.into());
        self
    }

    pub async fn is_healthy(&self) -> bool {
        self.http
            .get(format!("{}/health", self.endpoints.gateway()))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    pub async fn is_work_healthy(&self) -> bool {
        self.http
            .get(self.work_url("health"))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    pub async fn is_fully_healthy(&self) -> bool {
        self.is_healthy().await && self.is_work_healthy().await
    }

    pub async fn status(&self) -> Result<InfrastructureStatus> {
        self.get_json("api/status").await
    }

    pub async fn packages(&self) -> Result<PackageInventory> {
        self.get_json("api/packages").await
    }

    pub async fn create_checkout(&self, request: &CreateCheckout) -> Result<CheckoutLease> {
        self.post_work("v1/checkouts", request).await
    }

    pub async fn checkouts(&self) -> Result<Vec<CheckoutRecord>> {
        self.get_work("v1/checkouts").await
    }

    pub async fn checkout(&self, checkout_id: &str) -> Result<CheckoutRecord> {
        self.get_work(&format!("v1/checkouts/{checkout_id}")).await
    }

    pub async fn receipt(&self, receipt_id: &str) -> Result<WorkflowReceipt> {
        self.get_work(&format!("v1/receipts/{receipt_id}")).await
    }

    pub async fn renew_lease(
        &self,
        checkout_id: &str,
        request: &LeaseRequest,
    ) -> Result<CheckoutRecord> {
        self.post_work(&format!("v1/checkouts/{checkout_id}/lease/renew"), request)
            .await
    }

    pub async fn release_lease(
        &self,
        checkout_id: &str,
        request: &LeaseRequest,
    ) -> Result<CheckoutRecord> {
        self.post_work(
            &format!("v1/checkouts/{checkout_id}/lease/release"),
            request,
        )
        .await
    }

    pub async fn transition(
        &self,
        checkout_id: &str,
        request: &TransitionRequest,
    ) -> Result<CheckoutRecord> {
        self.post_work(&format!("v1/checkouts/{checkout_id}/transition"), request)
            .await
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}/{}", self.endpoints.gateway(), path);
        let mut request = self.http.get(&url);
        if let Some(token) = &self.read_token {
            request = request.bearer_auth(token);
        }
        request
            .send()
            .await
            .with_context(|| format!("failed to connect to package infrastructure at {url}"))?
            .error_for_status()
            .with_context(|| format!("package infrastructure rejected GET {url}"))?
            .json()
            .await
            .with_context(|| format!("package infrastructure returned invalid JSON from {url}"))
    }

    async fn get_work<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.work_url(path);
        let token = self
            .read_token
            .as_ref()
            .or(self.controller_token.as_ref())
            .context("package workflow read credential is unavailable")?;
        self.http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("failed to connect to package workflow at {url}"))?
            .error_for_status()
            .with_context(|| format!("package workflow rejected GET {url}"))?
            .json()
            .await
            .with_context(|| format!("package workflow returned invalid JSON from {url}"))
    }

    async fn post_work<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let Some(token) = self.controller_token.as_ref() else {
            bail!("package workflow controller credential is unavailable");
        };
        let url = self.work_url(path);
        self.http
            .post(&url)
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("failed to connect to package workflow at {url}"))?
            .error_for_status()
            .with_context(|| format!("package workflow rejected POST {url}"))?
            .json()
            .await
            .with_context(|| format!("package workflow returned invalid JSON from {url}"))
    }

    fn work_url(&self, path: &str) -> String {
        format!(
            "{}/work/{}",
            self.endpoints.gateway(),
            path.trim_start_matches('/')
        )
    }
}
