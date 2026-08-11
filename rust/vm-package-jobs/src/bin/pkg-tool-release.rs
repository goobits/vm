use anyhow::Result;
use clap::Parser;
use vm_package_jobs::release::tool::{release, ToolReleaseOptions};

#[derive(Parser)]
#[command(
    name = "pkg-tool-release",
    version,
    about = "Ephemeral immutable collection publisher"
)]
struct Cli {
    #[arg(long)]
    tool: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    release(ToolReleaseOptions { name: cli.tool }).await?;
    Ok(())
}
