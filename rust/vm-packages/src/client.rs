use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    BeginReleaseRequest, CheckoutLease, CheckoutRecord, CleanupRequest, CompleteReleaseRequest,
    ConsumerRecord, ConsumerUsage, CreateCheckout, CreateRollout, IntegrationRequest, LeaseRequest,
    PackageDefinition, PackageDrift, PublicationRequest, PublishToolArtifact, RegisterConsumer,
    RegisterPackage, RegisterTool, RegistryEndpoints, ReleaseRecord, ReleaseReworkRequest,
    ReviewRequest, RolloutRecord, RolloutValidationRequest, SubmissionRecord, ToolArtifactRecord,
    ToolDefinition, ToolIndex, ToolInventory, ToolPublicationReceipt, TransitionRequest,
    ValidationRequest, WorkflowReceipt,
};

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

    pub async fn packages(&self) -> Result<PackageInventory> {
        self.get_json("api/packages").await
    }

    pub async fn create_checkout(&self, request: &CreateCheckout) -> Result<CheckoutLease> {
        self.post_work("v1/checkouts", request).await
    }

    pub async fn register_package(&self, request: &RegisterPackage) -> Result<PackageDefinition> {
        self.post_work("v1/packages", request).await
    }

    pub async fn package_definitions(&self) -> Result<Vec<PackageDefinition>> {
        self.get_work("v1/packages").await
    }

    pub async fn package_definition(&self, name: &str) -> Result<PackageDefinition> {
        let name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
        self.get_work(&format!("v1/packages/{name}")).await
    }

    pub async fn register_tool(&self, request: &RegisterTool) -> Result<ToolDefinition> {
        self.post_work("v1/tools", request).await
    }

    pub async fn tools(&self) -> Result<Vec<ToolDefinition>> {
        self.get_work("v1/tools").await
    }

    pub async fn tool(&self, name: &str) -> Result<ToolInventory> {
        let name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
        self.get_work(&format!("v1/tools/{name}")).await
    }

    pub async fn resolve_tool(
        &self,
        name: &str,
        version: Option<&str>,
        target: &str,
    ) -> Result<ToolArtifactRecord> {
        let name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
        let query = {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            query.append_pair("target", target);
            if let Some(version) = version {
                query.append_pair("version", version);
            }
            query.finish()
        };
        let path = format!("v1/tools/{name}/resolve?{query}");
        self.get_work(&path).await
    }

    pub async fn tool_index(&self, target: &str) -> Result<ToolIndex> {
        let query = {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            query.append_pair("target", target);
            query.finish()
        };
        let path = format!("v1/tools/index?{query}");
        self.get_work(&path).await
    }

    pub async fn publish_tool_artifact(
        &self,
        name: &str,
        request: &PublishToolArtifact,
    ) -> Result<ToolArtifactRecord> {
        let name = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>();
        self.post_release(&format!("v1/tools/{name}/artifacts"), request)
            .await
    }

    pub async fn tool_receipt(&self, receipt_id: &str) -> Result<ToolPublicationReceipt> {
        self.get_work(&format!("v1/tool-receipts/{receipt_id}"))
            .await
    }

    pub fn tool_artifact_url(&self, artifact: &ToolArtifactRecord) -> String {
        format!("{}{}", self.endpoints.gateway(), artifact.artifact_path)
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

    pub async fn submissions(&self) -> Result<Vec<SubmissionRecord>> {
        self.get_work("v1/submissions").await
    }

    pub async fn submission(&self, submission_id: &str) -> Result<SubmissionRecord> {
        self.get_work(&format!("v1/submissions/{submission_id}"))
            .await
    }

    pub async fn checkout_submission(&self, checkout_id: &str) -> Result<SubmissionRecord> {
        self.get_work(&format!("v1/checkouts/{checkout_id}/submission"))
            .await
    }

    pub async fn validate_submission(
        &self,
        submission_id: &str,
        request: &ValidationRequest,
    ) -> Result<SubmissionRecord> {
        self.post_work(&format!("v1/submissions/{submission_id}/validate"), request)
            .await
    }

    pub async fn record_review(
        &self,
        submission_id: &str,
        request: &ReviewRequest,
    ) -> Result<SubmissionRecord> {
        self.post_authenticated(
            &format!("v1/submissions/{submission_id}/review"),
            request,
            self.reviewer_token
                .as_deref()
                .or(self.controller_token.as_deref()),
            "reviewer",
        )
        .await
    }

    pub async fn next_review(&self) -> Result<Option<SubmissionRecord>> {
        self.get_authenticated(
            "v1/jobs/review/next",
            self.reviewer_token.as_deref(),
            "reviewer",
        )
        .await
    }

    pub async fn prepare_integration(
        &self,
        submission_id: &str,
        request: &IntegrationRequest,
    ) -> Result<SubmissionRecord> {
        self.post_work(
            &format!("v1/submissions/{submission_id}/integrate"),
            request,
        )
        .await
    }

    pub async fn complete_integration(
        &self,
        submission_id: &str,
        request: &ValidationRequest,
    ) -> Result<SubmissionRecord> {
        self.post_work(
            &format!("v1/submissions/{submission_id}/integration/complete"),
            request,
        )
        .await
    }

    pub async fn releases(&self) -> Result<Vec<ReleaseRecord>> {
        self.get_work("v1/releases").await
    }

    pub async fn release(&self, release_id: &str) -> Result<ReleaseRecord> {
        self.get_work(&format!("v1/releases/{release_id}")).await
    }

    pub async fn next_release(&self) -> Result<Option<SubmissionRecord>> {
        self.get_authenticated(
            "v1/jobs/release/next",
            self.release_token.as_deref(),
            "release",
        )
        .await
    }

    pub async fn begin_release(
        &self,
        submission_id: &str,
        request: &BeginReleaseRequest,
    ) -> Result<ReleaseRecord> {
        self.post_release(&format!("v1/submissions/{submission_id}/release"), request)
            .await
    }

    pub async fn request_release_rework(
        &self,
        submission_id: &str,
        request: &ReleaseReworkRequest,
    ) -> Result<SubmissionRecord> {
        self.post_release(
            &format!("v1/submissions/{submission_id}/release/rework"),
            request,
        )
        .await
    }

    pub async fn record_publication(
        &self,
        release_id: &str,
        request: &PublicationRequest,
    ) -> Result<ReleaseRecord> {
        self.post_release(&format!("v1/releases/{release_id}/publications"), request)
            .await
    }

    pub async fn complete_release(
        &self,
        release_id: &str,
        request: &CompleteReleaseRequest,
    ) -> Result<ReleaseRecord> {
        self.post_release(&format!("v1/releases/{release_id}/complete"), request)
            .await
    }

    pub async fn cleanup_release(
        &self,
        release_id: &str,
        request: &CleanupRequest,
    ) -> Result<CheckoutRecord> {
        self.post_release(&format!("v1/releases/{release_id}/cleanup"), request)
            .await
    }

    pub async fn cleanup_checkout(
        &self,
        checkout_id: &str,
        request: &CleanupRequest,
    ) -> Result<CheckoutRecord> {
        self.post_work(&format!("v1/checkouts/{checkout_id}/cleanup"), request)
            .await
    }

    pub async fn register_consumer(&self, request: &RegisterConsumer) -> Result<ConsumerRecord> {
        self.post_work("v1/consumers", request).await
    }

    pub async fn consumers(&self) -> Result<Vec<ConsumerRecord>> {
        self.get_work("v1/consumers").await
    }

    pub async fn package_consumers(&self, package: &str) -> Result<Vec<ConsumerUsage>> {
        let package = url::form_urlencoded::byte_serialize(package.as_bytes()).collect::<String>();
        self.get_work(&format!("v1/consumers/by-package/{package}"))
            .await
    }

    pub async fn drift(&self) -> Result<Vec<PackageDrift>> {
        self.get_work("v1/drift").await
    }

    pub async fn create_rollout(&self, request: &CreateRollout) -> Result<RolloutRecord> {
        self.post_work("v1/rollouts", request).await
    }

    pub async fn rollouts(&self) -> Result<Vec<RolloutRecord>> {
        self.get_work("v1/rollouts").await
    }

    pub async fn rollout(&self, rollout_id: &str) -> Result<RolloutRecord> {
        self.get_work(&format!("v1/rollouts/{rollout_id}")).await
    }

    pub async fn reconcile_rollout_queue(&self) -> Result<Option<RolloutRecord>> {
        self.post_rollout("v1/jobs/rollout/reconcile", &()).await
    }

    pub async fn complete_rollout(
        &self,
        rollout_id: &str,
        request: &RolloutValidationRequest,
    ) -> Result<RolloutRecord> {
        self.post_rollout(&format!("v1/rollouts/{rollout_id}/complete"), request)
            .await
    }

    pub fn checkout_archive_url(&self, checkout_id: &str, consumer: &str) -> String {
        let consumer =
            url::form_urlencoded::byte_serialize(consumer.as_bytes()).collect::<String>();
        self.work_url(&format!(
            "v1/checkouts/{checkout_id}/archive?consumer={consumer}"
        ))
    }

    pub fn submission_upload_url(&self, checkout_id: &str, consumer: &str) -> String {
        let consumer =
            url::form_urlencoded::byte_serialize(consumer.as_bytes()).collect::<String>();
        self.work_url(&format!(
            "v1/checkouts/{checkout_id}/submission?consumer={consumer}"
        ))
    }

    pub fn integration_bundle_url(&self, submission_id: &str, consumer: &str) -> String {
        let consumer =
            url::form_urlencoded::byte_serialize(consumer.as_bytes()).collect::<String>();
        self.work_url(&format!(
            "v1/submissions/{submission_id}/integration?consumer={consumer}"
        ))
    }

    pub fn release_bundle_url(&self, submission_id: &str) -> String {
        self.work_url(&format!("v1/submissions/{submission_id}/release-bundle"))
    }

    pub fn rollout_bundle_url(&self, rollout_id: &str) -> String {
        self.work_url(&format!("v1/rollouts/{rollout_id}/bundle"))
    }

    pub fn rollout_upload_url(&self, rollout_id: &str) -> String {
        self.work_url(&format!("v1/rollouts/{rollout_id}/submission"))
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
            .or(self.agent_token.as_ref())
            .or(self.controller_token.as_ref())
            .or(self.reviewer_token.as_ref())
            .or(self.release_token.as_ref())
            .or(self.rollout_token.as_ref())
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

    async fn get_authenticated<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        token: Option<&str>,
        scope: &str,
    ) -> Result<T> {
        let token = token
            .or(self.controller_token.as_deref())
            .with_context(|| format!("package workflow {scope} credential is unavailable"))?;
        let url = self.work_url(path);
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
        self.post_authenticated(
            path,
            body,
            self.controller_token
                .as_deref()
                .or(self.agent_token.as_deref()),
            "agent or controller",
        )
        .await
    }

    async fn post_release<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_authenticated(
            path,
            body,
            self.release_token
                .as_deref()
                .or(self.controller_token.as_deref()),
            "release",
        )
        .await
    }

    async fn post_rollout<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_authenticated(
            path,
            body,
            self.rollout_token
                .as_deref()
                .or(self.controller_token.as_deref()),
            "rollout",
        )
        .await
    }

    async fn post_authenticated<T, B>(
        &self,
        path: &str,
        body: &B,
        token: Option<&str>,
        scope: &str,
    ) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let token =
            token.with_context(|| format!("package workflow {scope} credential is unavailable"))?;
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
    }
}
