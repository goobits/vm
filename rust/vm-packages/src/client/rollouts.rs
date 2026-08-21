use anyhow::Result;

use crate::{
    ConsumerRecord, ConsumerUsage, CreateRollout, PackageDrift, RegisterConsumer, RolloutRecord,
    RolloutValidationRequest,
};

use super::PackageInfrastructureClient;

impl PackageInfrastructureClient {
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
        self.post_source_sync("v1/rollouts", request).await
    }

    pub async fn rollouts(&self) -> Result<Vec<RolloutRecord>> {
        self.get_work("v1/rollouts").await
    }

    pub async fn rollout(&self, rollout_id: &str) -> Result<RolloutRecord> {
        self.get_work(&format!("v1/rollouts/{rollout_id}")).await
    }

    pub async fn reconcile_rollout_queue(&self) -> Result<Option<RolloutRecord>> {
        self.post_rollout_sync("v1/jobs/rollout/reconcile", &())
            .await
    }

    pub async fn complete_rollout(
        &self,
        rollout_id: &str,
        request: &RolloutValidationRequest,
    ) -> Result<RolloutRecord> {
        self.post_rollout_sync(&format!("v1/rollouts/{rollout_id}/complete"), request)
            .await
    }

    pub fn rollout_bundle_url(&self, rollout_id: &str) -> String {
        self.work_url(&format!("v1/rollouts/{rollout_id}/bundle"))
    }

    pub fn rollout_upload_url(&self, rollout_id: &str) -> String {
        self.work_url(&format!("v1/rollouts/{rollout_id}/submission"))
    }
}
