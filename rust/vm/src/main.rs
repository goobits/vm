// Standard library
use std::sync::OnceLock;
use uuid::Uuid;

// External crates
use clap::Parser;
use log::info;

// Internal imports
use vm_common::messages::{messages::MESSAGES, msg};
use vm_common::{scoped_context, vm_error, vm_warning};

// Local modules
mod cli;
mod commands;

use cli::Args;
use commands::execute_command;

/// Request ID for this execution - used for tracing logs across the entire request
static REQUEST_ID: OnceLock<String> = OnceLock::new();

fn get_request_id() -> &'static str {
    REQUEST_ID.get_or_init(|| Uuid::new_v4().to_string())
}

fn main() {
    // Initialize structured logging system first, but only if not in test mode
    // Tests expect clean stdout output, so we disable logging for test runs
    if std::env::var("VM_TEST_MODE").is_err() && vm_common::structured_log::init().is_err() {
        vm_warning!("Failed to initialize structured logging, falling back to basic logging");
    }

    let args = Args::parse();

    // Set up request-level context that will be inherited by all logs
    let _request_guard = scoped_context! {
        "request_id" => get_request_id(),
        "command" => format!("{:?}", args.command),
        "debug" => args.debug
    };

    if args.debug {
        info!("Starting vm command");
    }

    // Execute the command and handle any top-level errors
    if let Err(e) = execute_command(args) {
        // Use the new messaging system to format the final error output
        vm_error!("{}", msg!(MESSAGES.error_generic, error = format!("{:?}", e)));
        std::process::exit(1);
    }
}

