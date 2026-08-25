use std::process::ExitCode;

use anyhow::{Context, Result};
use vm_logging::init_service_subscriber;
use vm_package_jobs::review_submission;
use vm_package_jobs::runtime::{run_job_worker, worker_main};
use vm_packages::{PackageInfrastructureClient, RegistryEndpoints};

#[tokio::main]
async fn main() -> ExitCode {
    let _guard = init_service_subscriber();
    worker_main("package_review", run()).await
}

async fn run() -> Result<()> {
    let gateway =
        std::env::var("PKG_REVIEW_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let token = std::env::var("PKG_REVIEW_TOKEN").context("PKG_REVIEW_TOKEN is required")?;
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(gateway)?)
        .with_reviewer_token(&token);
    run_job_worker(
        "poll_review_queue",
        "review",
        || client.next_review(),
        |submission| {
            let client = &client;
            let token = &token;
            async move { review_submission(client, token, &submission.submission_id).await }
        },
        |submission| submission.submission_id.clone(),
    )
    .await
}
