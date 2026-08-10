use anyhow::Result;
use clap::Parser;
use vm_package_jobs::release::package::{release, PackageReleaseOptions};

#[derive(Parser)]
#[command(
    name = "pkg-release",
    version,
    about = "Ephemeral deterministic package releaser"
)]
struct Cli {
    #[arg(long)]
    submission: String,
    #[arg(long)]
    push_source: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    release(PackageReleaseOptions {
        submission: cli.submission,
        push_source: cli.push_source,
    })
    .await
}
