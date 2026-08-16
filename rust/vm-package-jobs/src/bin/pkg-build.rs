use std::path::PathBuf;

use anyhow::{Context, Result};
use vm_package_jobs::release::tool::build_submission;
use vm_package_jobs::runtime::required_secret;
use vm_packages::{PackageInfrastructureClient, RegistryEndpoints};

#[tokio::main]
async fn main() -> Result<()> {
    let gateway =
        std::env::var("PKG_BUILD_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let build_token = required_secret("PKG_BUILD_TOKEN_FILE")?;
    let staging_root = PathBuf::from(
        std::env::var_os("PKG_BUILD_STAGING_ROOT").context("PKG_BUILD_STAGING_ROOT is required")?,
    );
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(&gateway)?)
        .with_build_token(build_token.clone());
    loop {
        match client.next_tool_build().await {
            Ok(Some(submission)) => {
                if let Err(error) =
                    build_submission(&client, &submission, &build_token, &gateway, &staging_root)
                        .await
                {
                    eprintln!("build {} failed: {error:#}", submission.submission_id);
                }
            }
            Ok(None) => {}
            Err(error) => eprintln!("build queue unavailable: {error:#}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
