//! Standalone package server CLI binary
//!
//! This binary is the registry data-plane process used by the managed appliance.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vm_package_server::run_server;

#[derive(Parser)]
#[command(name = "pkg-server")]
#[command(about = "Goobits Package Server - Multi-registry package server")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the package server
    Start {
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to bind to
        #[arg(long, default_value = "3080")]
        port: u16,

        /// Data directory for package storage
        #[arg(long, default_value = "./data")]
        data: PathBuf,
    },
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_level(false)
        .without_time()
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { host, port, data } => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(run_server(host, port, data))
        }
    }
}
