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
use vm_core::{vm_error, vm_hint};
use vm_logging::init_subscriber;

// Local modules
mod cli;
mod commands;
mod confirmation;
mod error;
mod presentation;
mod services;

use cli::{
    Args, Command, ConfigPresetSubcommand, ConfigSubcommand, PluginSubcommand, SnapshotSubcommand,
    SystemStorageSubcommand, SystemSubcommand, TunnelSubcommand,
};
use commands::execute_command;

/// Request ID for this execution - used for tracing logs across the entire request
static REQUEST_ID: OnceLock<String> = OnceLock::new();

fn get_request_id() -> &'static str {
    REQUEST_ID.get_or_init(|| Uuid::new_v4().to_string())
}

/// Executes the given command and handles top-level errors.
async fn run_command(args: Args) {
    let single_exec = matches!(
        &args.command,
        Command::Exec { environments, fleet, .. } if !fleet.fleet && environments.len() <= 1
    );
    let json_command = match &args.command {
        Command::Tunnels {
            command: TunnelSubcommand::List { json: true, .. },
        } => Some("tunnels list"),
        Command::Plugins {
            command: PluginSubcommand::List { json: true },
        } => Some("plugins list"),
        Command::Plugins {
            command: PluginSubcommand::Show { json: true, .. },
        } => Some("plugins show"),
        Command::System {
            command: SystemSubcommand::Info { json: true },
        } => Some("system info"),
        Command::System {
            command:
                SystemSubcommand::Storage {
                    command: SystemStorageSubcommand::List { json: true },
                },
        } => Some("system storage list"),
        Command::System {
            command:
                SystemSubcommand::Storage {
                    command: SystemStorageSubcommand::Remove { json: true, .. },
                },
        } => Some("system storage remove"),
        Command::Snapshots {
            command: SnapshotSubcommand::List { json: true, .. },
        } => Some("snapshots list"),
        Command::Snapshots {
            command: SnapshotSubcommand::Show { json: true, .. },
        } => Some("snapshots show"),
        Command::Snapshots {
            command: SnapshotSubcommand::Create { json: true, .. },
        } => Some("snapshots create"),
        Command::Snapshots {
            command: SnapshotSubcommand::Restore { json: true, .. },
        } => Some("snapshots restore"),
        Command::Snapshots {
            command: SnapshotSubcommand::Remove { json: true, .. },
        } => Some("snapshots remove"),
        Command::Snapshots {
            command: SnapshotSubcommand::Export { json: true, .. },
        } => Some("snapshots export"),
        Command::Snapshots {
            command: SnapshotSubcommand::Import { json: true, .. },
        } => Some("snapshots import"),
        Command::Config {
            command: ConfigSubcommand::Show { json: true, .. },
        } => Some("config show"),
        Command::Config {
            command: ConfigSubcommand::Get { json: true, .. },
        } => Some("config get"),
        Command::Config {
            command: ConfigSubcommand::Set { json: true, .. },
        } => Some("config set"),
        Command::Config {
            command: ConfigSubcommand::Unset { json: true, .. },
        } => Some("config unset"),
        Command::Config {
            command:
                ConfigSubcommand::Presets {
                    command: ConfigPresetSubcommand::Apply { json: true, .. },
                },
        } => Some("config presets apply"),
        Command::List { json: true, .. } => Some("list"),
        Command::Status { json: true, .. } => Some("status"),
        _ => None,
    };
    if json_command.is_some() {
        vm_core::output_macros::set_quiet(true);
    }
    let result = execute_command(args).await;
    if let Err(error) = result {
        if error.is_guest_exit() {
            std::process::exit(error.exit_code());
        }
        if error.is_reported() {
            std::process::exit(error.exit_code());
        }
        let error = if single_exec && error.exit_code() != 2 {
            error.exec_prelaunch()
        } else {
            error
        };
        tracing::error!(
            operation = "execute_command",
            outcome = "failed",
            error = %error,
            error_source = error.source_chain().unwrap_or_default(),
            hint = error.hint().unwrap_or_default(),
            "vm command failed"
        );
        if let Some(command) = json_command {
            if let Err(output_error) = presentation::failure(command, &error) {
                vm_error!("Error: [operation_failed] {}", output_error);
            }
        } else {
            vm_error!("Error: [{}] {}", error.code(), error);
            if let Some(source) = error
                .source_chain()
                .filter(|source| source != &error.to_string())
            {
                vm_error!("Cause: {}", source);
            }
            if let Some(hint) = error.hint() {
                vm_hint!("{}", hint);
            }
        }
        std::process::exit(error.exit_code());
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
    if args.no_color {
        std::env::set_var("NO_COLOR", "1");
    }
    vm_core::output_macros::set_quiet(args.quiet);
    // The guard must be kept in scope for the lifetime of the application
    // to ensure that all buffered logs are flushed to the file.
    let _guard = init_subscriber();

    if std::env::var("VM_TEST_MODE").is_err() {
        let span = info_span!(
            "request",
            component = "vm_cli",
            request_id = %get_request_id()
        );
        run_command(args).instrument(span).await;
    } else {
        run_command(args).await;
    }
}
