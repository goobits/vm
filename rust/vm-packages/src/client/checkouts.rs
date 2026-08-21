use anyhow::Result;

use crate::{
    CheckoutLease, CheckoutRecord, CleanupRequest, CreateCheckout, LeaseRequest, TransitionRequest,
    WorkflowReceipt,
};

use super::PackageInfrastructureClient;

impl PackageInfrastructureClient {
    pub async fn create_checkout(&self, request: &CreateCheckout) -> Result<CheckoutLease> {
        self.post_source_sync("v1/checkouts", request).await
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

    pub async fn cleanup_checkout(
        &self,
        checkout_id: &str,
        request: &CleanupRequest,
    ) -> Result<CheckoutRecord> {
        self.post_work(&format!("v1/checkouts/{checkout_id}/cleanup"), request)
            .await
    }

    pub fn checkout_archive_url(&self, checkout_id: &str, consumer: &str) -> String {
        let consumer =
            url::form_urlencoded::byte_serialize(consumer.as_bytes()).collect::<String>();
        self.work_url(&format!(
            "v1/checkouts/{checkout_id}/archive?consumer={consumer}"
        ))
    }
}
