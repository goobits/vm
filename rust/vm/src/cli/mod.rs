// CLI argument parsing and definitions

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

mod subcommands;
pub use subcommands::*;

#[derive(Debug, Clone, Parser)]
#[command(name = "vm")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "Goobits VM Contributors")]
#[command(about = "Humane virtual environments")]
#[command(before_help = format!(" \nvm v{}", env!("CARGO_PKG_VERSION")))]
#[command(after_help = " \nRun `vm help <command>` for specific options.\n")]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    /// Path to a custom VM configuration file
    #[arg(long, global = true, conflicts_with = "project")]
    pub config: Option<PathBuf>,

    /// Select a registered project ID or a directory containing vm.yaml
    #[arg(long, global = true, conflicts_with = "config")]
    pub project: Option<PathBuf>,

    /// Select a configuration profile to apply
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// Disable colored terminal output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Suppress progress messages
    #[arg(long, global = true)]
    pub quiet: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExecOutput {
    Grouped,
    JsonLines,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Initialize a project in the current or selected directory
    Init {
        path: Option<PathBuf>,
        /// Development preset to initialize with
        #[arg(long, default_value = "vibe")]
        preset: String,
    },
    /// Declare and provision a stopped environment
    Create {
        name: String,
        #[arg(long, value_parser = vm_config::config::ProviderName::SUPPORTED)]
        provider: String,
        #[arg(
            long,
            required_unless_present = "snapshot",
            conflicts_with = "snapshot"
        )]
        image: Option<String>,
        #[arg(long, required_unless_present = "image")]
        snapshot: Option<String>,
        #[arg(long)]
        cpu: Option<String>,
        #[arg(long)]
        memory: Option<String>,
        #[arg(long)]
        mount: Vec<String>,
    },
    /// Start an existing environment
    Start {
        /// Environment names; omit to use the project default
        #[arg(conflicts_with = "fleet")]
        environments: Vec<String>,
        /// Return after requesting startup instead of waiting for readiness
        #[arg(long)]
        no_wait: bool,
        #[command(flatten)]
        fleet: FleetArgs,
    },
    /// List environments for this project
    List {
        /// Show environments across all projects
        #[arg(long)]
        all_projects: bool,
        /// Show provider IDs and raw provider names
        #[arg(long)]
        raw: bool,
        /// Emit one machine-readable JSON envelope
        #[arg(long)]
        json: bool,
    },
    /// Provision/start an initialized environment and open a shell
    Shell {
        /// Environment name; omit to use the project default
        environment: Option<String>,
        /// Directory path to start shell in
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    /// Run a single command inside an environment
    Exec {
        /// Run in these environments instead of the project default
        #[arg(long = "env", conflicts_with = "fleet")]
        environments: Vec<String>,
        #[command(flatten)]
        fleet: FleetArgs,
        /// Guest working directory
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Guest user name
        #[arg(long)]
        user: Option<String>,
        /// Required when executing in multiple environments
        #[arg(long, value_enum)]
        output: Option<ExecOutput>,
        #[arg(last = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// Stream output logs from an environment
    Logs {
        environment: Option<String>,
        /// Emit typed JSON Lines records and a final result event
        #[arg(long)]
        json_lines: bool,
        #[arg(short = 'f', long)]
        follow: bool,
        #[arg(short = 'n', long, default_value = "50")]
        tail: usize,
        #[arg(short = 's', long)]
        service: Option<String>,
    },
    /// Move files between host and environment
    Copy {
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        overwrite: bool,
        source: String,
        destination: String,
    },
    /// Gracefully halt an environment
    Stop {
        #[arg(conflicts_with = "fleet")]
        environments: Vec<String>,
        #[command(flatten)]
        fleet: FleetArgs,
    },
    /// Check environment status
    Status {
        /// Environment names; omit to use the project default
        #[arg(conflicts_with = "fleet")]
        environments: Vec<String>,
        #[command(flatten)]
        fleet: FleetArgs,
        /// Emit one machine-readable JSON envelope
        #[arg(long)]
        json: bool,
    },
    /// Stop and start an environment
    Restart {
        #[arg(conflicts_with = "fleet")]
        environments: Vec<String>,
        #[command(flatten)]
        fleet: FleetArgs,
    },
    /// Remove environments while preserving persistent data and snapshots
    Remove {
        #[arg(conflicts_with = "fleet")]
        environments: Vec<String>,
        #[command(flatten)]
        fleet: FleetArgs,
        /// Delete exclusively owned persistent environment data too
        #[arg(long)]
        delete_data: bool,
        /// Confirm the displayed target set without prompting
        #[arg(long)]
        yes: bool,
    },
    /// Manage environment snapshots and portable archives
    Snapshots {
        #[command(subcommand)]
        command: SnapshotSubcommand,
    },
    /// Manage the shared package-infrastructure appliance
    Packages {
        #[command(subcommand)]
        command: PackagesSubcommand,
    },
    /// Manage immutable tools activated inside project environments
    Tools {
        #[command(subcommand)]
        command: ToolsSubcommand,
    },
    /// Manage defaults, providers, and profiles
    Config {
        #[command(subcommand)]
        command: ConfigSubcommand,
    },
    /// Manage active port forwards
    Tunnels {
        #[command(subcommand)]
        command: TunnelSubcommand,
    },
    /// Diagnose and repair engine issues
    Doctor {
        /// Environment to diagnose
        environment: Option<String>,
        #[arg(long)]
        fix: bool,
        #[arg(long)]
        clean: bool,
        /// Prune unreferenced packages from an environment's pnpm store
        #[arg(long)]
        prune_pnpm_store: bool,
    },
    /// Extend with plugins
    Plugins {
        #[command(subcommand)]
        command: PluginSubcommand,
    },
    /// Self-management and lower-level system tools
    System {
        #[command(subcommand)]
        command: SystemSubcommand,
    },
    /// Database workflows
    Db {
        #[command(subcommand)]
        command: DbSubcommand,
    },
    /// Secret workflows
    Secrets {
        #[command(subcommand)]
        command: SecretSubcommand,
    },
    #[command(hide = true)]
    InternalCompletion { shell: String },
}

#[cfg(test)]
mod tests;
