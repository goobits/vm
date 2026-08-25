use anyhow::{Context, Result};
use vm_package_jobs::review_submission;
use vm_packages::{PackageInfrastructureClient, RegistryEndpoints};

#[tokio::main]
async fn main() -> Result<()> {
    let gateway =
        std::env::var("PKG_REVIEW_GATEWAY").unwrap_or_else(|_| "http://gateway:8080".into());
    let token = std::env::var("PKG_REVIEW_TOKEN").context("PKG_REVIEW_TOKEN is required")?;
    let client = PackageInfrastructureClient::new(RegistryEndpoints::new(gateway)?)
        .with_reviewer_token(&token);

    loop {
        match client.next_review().await {
            Ok(Some(submission)) => {
                if let Err(error) =
                    review_submission(&client, &token, &submission.submission_id).await
                {
                    eprintln!("review {} failed: {error:#}", submission.submission_id);
                }
            }
            Ok(None) => {}
            Err(error) => eprintln!("review queue unavailable: {error:#}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
