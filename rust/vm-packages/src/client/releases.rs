use anyhow::Result;

use crate::{
    BeginReleaseRequest, CheckoutRecord, CleanupRequest, CompleteReleaseRequest,
    PublicationRequest, ReleaseRecord, ReleaseReworkRequest, SubmissionRecord,
};

use super::PackageInfrastructureClient;

impl PackageInfrastructureClient {
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

    pub fn release_bundle_url(&self, submission_id: &str) -> String {
        self.work_url(&format!("v1/submissions/{submission_id}/release-bundle"))
    }
}
