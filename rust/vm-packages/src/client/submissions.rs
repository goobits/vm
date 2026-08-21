use anyhow::Result;

use crate::{IntegrationRequest, ReviewRequest, SubmissionRecord, ValidationRequest};

use super::PackageInfrastructureClient;

impl PackageInfrastructureClient {
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
        self.post_source_sync(
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

    pub fn review_bundle_url(&self, submission_id: &str) -> String {
        self.work_url(&format!("v1/submissions/{submission_id}/review-bundle"))
    }
}
