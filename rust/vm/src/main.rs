//! VM Management Tool
//!
//! A fast, portable, and modern command-line tool for managing virtual machines across
//! multiple providers (Docker, Podman, Tart). Provides a unified interface for creating,
//! starting, stopping, and managing development environments.

// Standard library
use std::sync::OnceLock;
use uuid::Uuid;

// External crates
use clap::Parser;
use tracing::info_span;
use tracing::Instrument;

// Internal imports
use vm_core::vm_error;
use vm_logging::init_subscriber;

// Local modules
mod cli;
mod commands;
mod error;
mod service_manager;
mod service_registry;
mod services;
mod state;
mod utils;

use cli::Args;
use commands::execute_command;

/// Request ID for this execution - used for tracing logs across the entire request
static REQUEST_ID: OnceLock<String> = OnceLock::new();

fn get_request_id() -> &'static str {
    REQUEST_ID.get_or_init(|| Uuid::new_v4().to_string())
}

/// Executes the given command and handles top-level errors.
async fn run_command(args: Args) {
    if let Err(e) = execute_command(args).await {
        vm_error!("{}", e);
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() {
    // Auto-detect CI environment
    if std::env::var("CI").is_ok() {
        // Disable colors and interactive elements
        std::env::set_var("NO_COLOR", "1");
    }

    let args = Args::parse();
    // The guard must be kept in scope for the lifetime of the application
    // to ensure that all buffered logs are flushed to the file.
    let _guard = init_subscriber();

    if std::env::var("VM_TEST_MODE").is_err() {
        let span = info_span!("request",
            request_id = %get_request_id(),
            command = ?args.command
        );
        run_command(args).instrument(span).await;
    } else {
        run_command(args).await;
    }
}
