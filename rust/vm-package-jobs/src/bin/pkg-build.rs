use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use vm_logging::init_service_subscriber;
use vm_package_jobs::release::tool::build_submission;
use vm_package_jobs::runtime::{required_secret, worker_main, QueueMonitor};
use vm_packages::{PackageInfrastructureClient, RegistryEndpoints};

#[tokio::main]
async fn main() -> ExitCode {
    let _guard = init_service_subscriber();
    worker_main("package_build", run()).await
}

async fn run() -> Result<()> {
    let gateway =
        std::env::var("PKG_BUILD_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let build_token = required_secret("PKG_BUILD_TOKEN_FILE")?;
    let staging_root = PathBuf::from(
        std::env::var_os("PKG_BUILD_STAGING_ROOT").context("PKG_BUILD_STAGING_ROOT is required")?,
    );
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(&gateway)?)
        .with_build_token(build_token.clone());
    let mut queue = QueueMonitor::new("poll_build_queue");
    loop {
        match client.next_tool_build().await {
            Ok(Some(submission)) => {
                queue.available();
                if let Err(error) =
                    build_submission(&client, &submission, &build_token, &gateway, &staging_root)
                        .await
                {
                    tracing::error!(
                        operation = "build",
                        submission_id = %submission.submission_id,
                        error = ?error,
                        "package build failed"
                    );
                }
            }
            Ok(None) => queue.available(),
            Err(error) => queue.unavailable(&error),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
