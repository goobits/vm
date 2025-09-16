// Standard library
use uuid::Uuid;

// External crates
use anyhow::Result;
use clap::Parser;
use log::info;

// Internal imports
use vm_common::scoped_context;

// Local modules
mod cli;
mod commands;

use cli::Args;
use commands::execute_command;

// Request ID for this execution - used for tracing logs across the entire request
static REQUEST_ID: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| Uuid::new_v4().to_string());

fn main() -> Result<()> {
    // Initialize structured logging system first, but only if not in test mode
    // Tests expect clean stdout output, so we disable logging for test runs
    if std::env::var("VM_TEST_MODE").is_err() && vm_common::structured_log::init().is_err() {
        eprintln!(
            "Warning: Failed to initialize structured logging, falling back to basic logging"
        );
    }

    let args = Args::parse();

    // Set up request-level context that will be inherited by all logs
    let _request_guard = scoped_context! {
        "request_id" => REQUEST_ID.as_str(),
        "command" => format!("{:?}", args.command),
        "debug" => args.debug
    };

    info!("Starting vm command");

    // Execute the command using the new command dispatcher
    execute_command(args)
}
