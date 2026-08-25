use std::process::ExitCode;

use anyhow::Result;
use vm_logging::init_service_subscriber;
use vm_package_jobs::release::package::{release, PackageReleaseOptions};
use vm_package_jobs::runtime::{
    required_secret, worker_main, JobMonitor, QueueMonitor, POLL_INTERVAL,
};
use vm_packages::{PackageInfrastructureClient, RegistryEndpoints};

#[tokio::main]
async fn main() -> ExitCode {
    let _guard = init_service_subscriber();
    worker_main("package_release", run()).await
}

async fn run() -> Result<()> {
    let gateway =
        std::env::var("PKG_RELEASE_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(gateway)?)
        .with_release_token(required_secret("PKG_RELEASE_TOKEN_FILE")?);
    let mut queue = QueueMonitor::new("poll_release_queue");
    let mut jobs = JobMonitor::new("release");
    loop {
        let delay = match client.next_release().await {
            Ok(Some(submission)) => {
                queue.available();
                match release(PackageReleaseOptions {
                    submission: submission.submission_id.clone(),
                })
                .await
                {
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
