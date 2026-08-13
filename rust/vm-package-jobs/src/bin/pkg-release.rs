use anyhow::Result;
use vm_package_jobs::release::package::{release, PackageReleaseOptions};
use vm_package_jobs::runtime::required_secret;
use vm_packages::{PackageInfrastructureClient, RegistryEndpoints};

#[tokio::main]
async fn main() -> Result<()> {
    let gateway =
        std::env::var("PKG_RELEASE_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(gateway)?)
        .with_release_token(required_secret("PKG_RELEASE_TOKEN_FILE")?);
    loop {
        match client.next_release().await {
            Ok(Some(submission)) => {
                if let Err(error) = release(PackageReleaseOptions {
                    submission: submission.submission_id.clone(),
                })
                .await
                {
                    eprintln!("release {} failed: {error:#}", submission.submission_id);
                }
            }
            Ok(None) => {}
            Err(error) => eprintln!("release queue unavailable: {error:#}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
