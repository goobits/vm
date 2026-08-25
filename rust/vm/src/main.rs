//! VM Management Tool
//!
//! A fast, portable, and modern command-line tool for managing virtual machines across
//! multiple providers (Docker, Podman, Tart). Provides a unified interface for creating,
//! starting, stopping, and managing development environments.

// Standard library
use std::sync::OnceLock;
use uuid::Uuid;

// External crates
use clap::{error::ErrorKind, CommandFactory, Parser};
use tracing::info_span;
use tracing::Instrument;

// Internal imports
use vm_core::{vm_error, vm_hint};
use vm_logging::init_subscriber;

// Local modules
mod cli;
mod commands;
mod error;
mod service_manager;
mod services;
mod utils;

use cli::Args;
use commands::execute_command;

enum Invocation {
    BuiltIn(Box<Args>),
    Remote(Vec<std::ffi::OsString>),
}

/// Request ID for this execution - used for tracing logs across the entire request
static REQUEST_ID: OnceLock<String> = OnceLock::new();

fn get_request_id() -> &'static str {
    REQUEST_ID.get_or_init(|| Uuid::new_v4().to_string())
}

/// Executes the given command and handles top-level errors.
async fn run_command(invocation: Invocation) {
    let result = match invocation {
        Invocation::BuiltIn(args) => execute_command(*args).await,
        Invocation::Remote(arguments) => commands::remote_command::handle(arguments).await,
    };
    if let Err(error) = result {
        vm_error!("Error: {}", error);
        if let Some(hint) = error.hint() {
            vm_hint!("{}", hint);
        }
        std::process::exit(1);
    }
}

fn parse_invocation() -> Invocation {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    match Args::try_parse_from(&arguments) {
        Ok(args) => Invocation::BuiltIn(Box::new(args)),
        Err(error)
            if error.kind() == ErrorKind::InvalidSubcommand
                && top_level_namespace(&arguments).is_some_and(|namespace| {
                    !Args::command().get_subcommands().any(|command| {
                        command.get_name() == namespace
                            || command.get_all_aliases().any(|alias| alias == namespace)
                    })
                }) =>
        {
            Invocation::Remote(arguments.into_iter().skip(1).collect())
        }
        Err(error) => error.exit(),
    }
}

fn top_level_namespace(arguments: &[std::ffi::OsString]) -> Option<&str> {
    let mut index = 1;
    while let Some(argument) = arguments.get(index).and_then(|value| value.to_str()) {
        match argument {
            "--config" | "--profile" => index += 2,
            "--dry-run" => index += 1,
            value if value.starts_with("--config=") || value.starts_with("--profile=") => {
                index += 1;
            }
            value if value.starts_with('-') => return None,
            value => return Some(value),
        }
    }
    None
}

#[tokio::main]
async fn main() {
    // Auto-detect CI environment
    if std::env::var("CI").is_ok() {
        // Disable colors and interactive elements
        std::env::set_var("NO_COLOR", "1");
    }

    let invocation = parse_invocation();
    // The guard must be kept in scope for the lifetime of the application
    // to ensure that all buffered logs are flushed to the file.
    let _guard = init_subscriber();

    if std::env::var("VM_TEST_MODE").is_err() {
        let span = info_span!("request", request_id = %get_request_id());
        run_command(invocation).instrument(span).await;
    } else {
        run_command(invocation).await;
    }
}
