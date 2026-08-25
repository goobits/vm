use std::process::ExitCode;

use anyhow::Result;
use vm_logging::init_service_subscriber;
use vm_package_jobs::release::package::{release, PackageReleaseOptions};
use vm_package_jobs::runtime::{required_secret, worker_main, QueueMonitor};
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
    loop {
        match client.next_release().await {
            Ok(Some(submission)) => {
                queue.available();
                if let Err(error) = release(PackageReleaseOptions {
                    submission: submission.submission_id.clone(),
                })
                .await
                {
                    tracing::error!(
                        operation = "release",
                        submission_id = %submission.submission_id,
                        error = ?error,
                        "package release failed"
                    );
                }
            }
            Ok(None) => queue.available(),
            Err(error) => queue.unavailable(&error),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
