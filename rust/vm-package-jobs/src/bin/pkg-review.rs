use std::process::ExitCode;

use anyhow::{Context, Result};
use vm_logging::init_service_subscriber;
use vm_package_jobs::review_submission;
use vm_package_jobs::runtime::{worker_main, JobMonitor, QueueMonitor, POLL_INTERVAL};
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
    let mut jobs = JobMonitor::new("review");

    loop {
        let delay = match client.next_review().await {
            Ok(Some(submission)) => {
                queue.available();
                match review_submission(&client, &token, &submission.submission_id).await {
                    Ok(()) => {
                        jobs.succeeded(&submission.submission_id);
                        POLL_INTERVAL
                    }
                    Err(error) => jobs.failed(&submission.submission_id, &error),
                }
            }
            Ok(None) => {
                queue.available();
                POLL_INTERVAL
            }
            Err(error) => {
                queue.unavailable(&error);
                POLL_INTERVAL
            }
        };
        tokio::time::sleep(delay).await;
    }
}
