//! Standalone package server CLI binary
//!
//! This binary provides the same commands as `vm pkg` but as a standalone `pkg-server` tool.
//! Both CLIs expose identical functionality for package server operations.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber;
use vm_package_server::{
    add_package, list_packages, remove_package, run_server, run_server_background, show_status,
};

#[derive(Parser)]
#[command(name = "pkg-server")]
#[command(about = "Goobits Package Server - Multi-registry package server")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Server URL for client operations
    #[arg(long, default_value = "http://localhost:3080", global = true)]
    server: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the package server
    Start {
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Port to bind to
        #[arg(long, default_value = "3080")]
        port: u16,

        /// Data directory for package storage
        #[arg(long, default_value = "./data")]
        data: PathBuf,
    },

    /// Start the package server in background
    Background {
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Port to bind to
        #[arg(long, default_value = "3080")]
        port: u16,

        /// Data directory for package storage
        #[arg(long, default_value = "./data")]
        data: PathBuf,
    },

    /// Add/publish package from current directory
    Add {
        /// Filter package types (e.g., "python,npm")
        #[arg(long)]
        r#type: Option<String>,
    },

    /// Remove/delete packages interactively
    Remove {
        /// Force removal without confirmation
        #[arg(long)]
        force: bool,
    },

    /// List all packages on the server
    List,

    /// Show server status and package counts
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { host, port, data } => run_server(host, port, data).await,

        Commands::Background { host, port, data } => run_server_background(host, port, data).await,

        Commands::Add { r#type } => add_package(&cli.server, r#type.as_deref()),

        Commands::Remove { force } => remove_package(&cli.server, force),

        Commands::List => list_packages(&cli.server),

        Commands::Status => show_status(&cli.server),
    }
}
