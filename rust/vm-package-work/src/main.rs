use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "pkg-work",
    version,
    about = "Deterministic package workflow service"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Start {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 3091)]
        port: u16,
        #[arg(long, default_value = "./data")]
        data: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    match Cli::parse().command {
        Command::Start { host, port, data } => {
            let read_token = std::env::var("PKG_WORK_READ_TOKEN")?;
            let controller_token = std::env::var("PKG_WORK_CONTROLLER_TOKEN")?;
            let reviewer_token = std::env::var("PKG_WORK_REVIEWER_TOKEN")?;
            tokio::runtime::Runtime::new()?.block_on(vm_package_work::run(
                host,
                port,
                data,
                read_token,
                controller_token,
                reviewer_token,
            ))?;
        }
    }
    Ok(())
}
