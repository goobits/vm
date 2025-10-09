//! VM Management Tool
//!
//! A fast, portable, and modern command-line tool for managing virtual machines across
//! multiple providers (Docker, Vagrant, Tart). Provides a unified interface for creating,
//! starting, stopping, and managing development environments.

// Standard library
use std::sync::OnceLock;
use uuid::Uuid;

// External crates
use clap::Parser;
use tracing::{info, info_span};
use tracing_subscriber::{prelude::*, EnvFilter};

// Internal imports
// use vm_core::messages::{messages::MESSAGES, msg}; // Currently unused
use vm_core::{vm_error, vm_warning};

// Local modules
mod cli;
mod commands;
mod error;
mod service_manager;
mod service_registry;

use cli::Args;
use commands::execute_command;

/// Request ID for this execution - used for tracing logs across the entire request
static REQUEST_ID: OnceLock<String> = OnceLock::new();

fn get_request_id() -> &'static str {
    REQUEST_ID.get_or_init(|| Uuid::new_v4().to_string())
}

#[tokio::main]
async fn main() {
    // Initialize tracing system.
    // In test mode, we skip initialization to keep test output clean.
    if std::env::var("VM_TEST_MODE").is_err() {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

        let use_json = std::env::var("VM_JSON_LOGS")
            .map(|val| val == "1" || val.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let subscriber = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_ansi(!use_json);

        let result = if use_json {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(subscriber.json())
                .try_init()
        } else {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(subscriber.pretty())
                .try_init()
        };

        if let Err(e) = result {
            vm_warning!("Failed to initialize tracing: {}", e);
        }
    }

    let args = Args::parse();

    // Set up request-level span that will be inherited by all logs
    let span = info_span!("request",
        request_id = %get_request_id(),
        command = ?args.command,
        debug = args.debug
    );
    let _enter = span.enter();

    if args.debug {
        info!("Starting vm command");
    }

    // Execute the command and handle any top-level errors
    if let Err(e) = execute_command(args).await {
        // Use the Display trait for user-friendly error messages
        vm_error!("{}", e);
        std::process::exit(1);
    }
}
