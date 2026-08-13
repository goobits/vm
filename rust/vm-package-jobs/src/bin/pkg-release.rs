use anyhow::{Context, Result};
use clap::Parser;
use vm_package_jobs::release::package::{release, PackageReleaseOptions};
use vm_package_jobs::runtime::required_secret;
use vm_packages::{PackageInfrastructureClient, RegistryEndpoints};

#[derive(Parser)]
#[command(
    name = "pkg-release",
    version,
    about = "Ephemeral deterministic package releaser"
)]
struct Cli {
    #[arg(long, required_unless_present = "watch")]
    submission: Option<String>,
    #[arg(long)]
    watch: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.watch {
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
    release(PackageReleaseOptions {
        submission: cli.submission.context("--submission is required")?,
    })
    .await
}
