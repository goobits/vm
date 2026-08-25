use std::process::ExitCode;

use anyhow::Result;
use vm_logging::init_service_subscriber;
use vm_package_jobs::release::package::{release, PackageReleaseOptions};
use vm_package_jobs::runtime::{required_secret, run_job_worker, worker_main};
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
    run_job_worker(
        "poll_release_queue",
        "release",
        || client.next_release(),
        |submission| {
            release(PackageReleaseOptions {
                submission: submission.submission_id,
            })
        },
        |submission| submission.submission_id.clone(),
    )
    .await
}
