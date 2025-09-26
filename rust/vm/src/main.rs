// Standard library
use std::sync::OnceLock;
use uuid::Uuid;

// External crates
use anyhow::Result;
use clap::Parser;
use log::info;

// Internal imports
use vm_common::messages::{messages::MESSAGES, msg};
use vm_common::{scoped_context, vm_error, vm_println, vm_warning};

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

fn main() -> Result<()> {
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

    // Execute the command using the new command dispatcher with error handling
    match execute_command(args.clone()) {
        Ok(_) => Ok(()),
        Err(error) => {
            handle_error(&error, &args);
            std::process::exit(1);
        }
    }
}

/// Centralized error handler that formats errors using the messaging system
fn handle_error(error: &anyhow::Error, args: &Args) {
    // Display the primary error using our centralized message system
    vm_error!(
        "{}",
        msg!(MESSAGES.error_with_context, error = error.to_string())
    );

    // Display the error chain for context
    let error_chain: Vec<String> = error
        .chain()
        .skip(1) // Skip the root error we already displayed
        .map(|e| e.to_string())
        .collect();

    if !error_chain.is_empty() {
        for cause in error_chain {
            vm_println!("  └─ {}", cause);
        }
    }

    // In debug mode, show additional debugging information
    if args.debug {
        vm_println!(
            "{}",
            msg!(MESSAGES.error_debug_info, details = format!("{:?}", error))
        );

        // Show backtrace if available
        let backtrace = error.backtrace();
        let backtrace_str = backtrace.to_string();
        if !backtrace_str.trim().is_empty() && backtrace_str != "disabled backtrace" {
            vm_println!("Backtrace:\n{}", backtrace_str);
        }
    }
}
