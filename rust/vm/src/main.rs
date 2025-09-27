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

// Internal imports
// use vm_common::messages::{messages::MESSAGES, msg}; // Currently unused
use vm_common::{vm_error, vm_warning};

// Local modules
mod cli;
mod commands;
mod error;

use cli::Args;
use commands::execute_command;

/// Request ID for this execution - used for tracing logs across the entire request
static REQUEST_ID: OnceLock<String> = OnceLock::new();

fn get_request_id() -> &'static str {
    REQUEST_ID.get_or_init(|| Uuid::new_v4().to_string())
}

#[tokio::main]
async fn main() {
    // Initialize tracing system first, but only if not in test mode
    // Tests expect clean stdout output, so we disable logging for test runs
    if std::env::var("VM_TEST_MODE").is_err() {
        if let Err(e) = vm_common::tracing_init::init_with_defaults("warn") {
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
