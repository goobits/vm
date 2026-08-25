use std::process::ExitCode;

use anyhow::{Context, Result};
use vm_logging::init_service_subscriber;
use vm_package_jobs::review_submission;
use vm_package_jobs::runtime::{worker_main, QueueMonitor};
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
    let mut queue = QueueMonitor::new("poll_review_queue");

    loop {
        match client.next_review().await {
            Ok(Some(submission)) => {
                queue.available();
                if let Err(error) =
                    review_submission(&client, &token, &submission.submission_id).await
                {
                    tracing::error!(
                        operation = "review",
                        submission_id = %submission.submission_id,
                        error = ?error,
                        "package review failed"
                    );
                }
            }
            Ok(None) => queue.available(),
            Err(error) => queue.unavailable(&error),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
