use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use vm_logging::init_service_subscriber;
use vm_package_jobs::release::tool::build_submission;
use vm_package_jobs::runtime::{required_secret, run_job_worker, worker_main};
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
    run_job_worker(
        "poll_build_queue",
        "build",
        || client.next_tool_build(),
        |submission| {
            let client = &client;
            let build_token = &build_token;
            let gateway = &gateway;
            let staging_root = &staging_root;
            async move {
                build_submission(client, &submission, build_token, gateway, staging_root).await
            }
        },
        |submission| submission.submission_id.clone(),
    )
    .await
}
