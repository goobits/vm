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
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Select a configuration profile to apply
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// Show what would be executed without running
    #[arg(long, global = true)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum EnvironmentKind {
    /// A macOS virtual machine powered by Tart
    Mac,
    /// A Linux development environment
    Linux,
    /// A generic container environment
    Container,
}

impl EnvironmentKind {
    pub fn default_provider(self) -> &'static str {
        match self {
            Self::Mac => "tart",
            Self::Linux | Self::Container => "docker",
        }
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Create/configure an environment from vm.yaml
    #[command(hide = true)]
    Create {
        environment: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Start an existing environment
    Start {
        /// Environment name, not provider; omit to use the project default
        environment: Option<String>,
        /// Return after requesting startup instead of waiting for readiness
        #[arg(long)]
        no_wait: bool,
    },
    /// Create and start an environment
    Run {
        /// Environment kind: mac, linux, or container
        kind: EnvironmentKind,
        /// Optional natural-language name: as <name>
        #[arg(num_args = 0..=2)]
        words: Vec<String>,
        /// Advanced provider override
        #[arg(long, value_parser = ["docker", "podman", "tart"])]
        provider: Option<String>,
        /// Use a specific image, distro, or snapshot name
        #[arg(long)]
        image: Option<String>,
        /// Build from a local Dockerfile or context
        #[arg(long, value_name = "PATH")]
        build: Option<PathBuf>,
        /// Clone from a saved snapshot
        #[arg(long = "from-snapshot")]
        from_snapshot: Option<String>,
        /// Remove when stopped/exited
        #[arg(long)]
        ephemeral: bool,
        /// Mount a local folder into the environment
        #[arg(long)]
        mount: Vec<String>,
        /// CPU limit
        #[arg(long)]
        cpu: Option<String>,
        /// Memory limit
        #[arg(long)]
        memory: Option<String>,
    },
    /// List environments for this project
    #[command(visible_alias = "ls")]
    List {
        /// Show environments across all projects
        #[arg(long)]
        all: bool,
        /// Show provider IDs and raw provider names
        #[arg(long)]
        raw: bool,
    },
    /// Open a shell promptly; safe runtime updates continue in the background
    #[command(visible_alias = "ssh")]
    Shell {
        /// Environment name; omit to use the project default
        environment: Option<String>,
        /// Directory path to start shell in
        #[arg(long)]
        path: Option<PathBuf>,
        /// Command to execute instead of opening an interactive shell
        #[arg(short = 'e', long = "command")]
        command: Option<String>,
    },
    /// Run a single command inside an environment
    Exec {
        /// Environment name; omit it before `--` to use and start the project default
        environment: Option<String>,
        #[arg(last = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// Stream output logs from an environment
    Logs {
        environment: Option<String>,
        #[arg(short = 'f', long)]
        follow: bool,
        #[arg(short = 'n', long, default_value = "50")]
        tail: usize,
        #[arg(short = 's', long)]
        service: Option<String>,
    },
    /// Move files between host and environment
    Copy { source: String, destination: String },
    /// Gracefully halt an environment
    #[command(alias = "down", alias = "halt")]
    Stop { environment: Option<String> },
    /// Check environment status
    Status {
        /// Environment name; defaults to this project's canonical environment
        environment: Option<String>,
    },
    /// Stop and start an environment
    Restart { environment: Option<String> },
    /// Remove an environment while preserving saved snapshots
    #[command(alias = "rm", alias = "destroy")]
    Remove {
        environment: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Save the current state of an environment
    Save {
        /// Either `as <snapshot>` or `<environment> as <snapshot>`
        #[arg(required = true, num_args = 2..=3)]
        words: Vec<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        quiesce: bool,
        #[arg(long)]
        force: bool,
    },
    /// Restore an environment to a saved state
    Revert {
        /// Either `<snapshot>` or `<environment> <snapshot>`
        #[arg(required = true, num_args = 1..=2)]
        words: Vec<String>,
        #[arg(long)]
        force: bool,
    },
    /// Export an environment or base as a portable artifact
    Package {
        environment: Option<String>,
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
        #[arg(long, default_value = "6")]
        compress: u8,
        /// Build package directly from a Dockerfile
        #[arg(long, value_name = "PATH")]
        build: Option<PathBuf>,
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
    Tunnel {
        #[command(subcommand)]
        command: TunnelSubcommand,
    },
    /// Diagnose and repair engine issues
    Doctor {
        #[arg(long)]
        fix: bool,
        #[arg(long)]
        clean: bool,
        /// Prune unreferenced packages from an environment's pnpm store
        #[arg(long)]
        prune_pnpm_store: bool,
        /// Environment to maintain
        #[arg(long, requires = "prune_pnpm_store")]
        container: Option<String>,
    },
    /// Extend with plugins
    Plugin {
        #[command(subcommand)]
        command: PluginSubcommand,
    },
    /// Self-management and lower-level system tools
    System {
        #[command(subcommand)]
        command: SystemSubcommand,
    },
    /// Plugin-backed database workflows
    Db {
        #[command(subcommand)]
        command: DbSubcommand,
    },
    /// Plugin-backed fleet workflows
    Fleet {
        #[command(subcommand)]
        command: FleetSubcommand,
    },
    /// Plugin-backed secret workflows
    Secret {
        #[command(subcommand)]
        command: SecretSubcommand,
    },
    #[command(hide = true)]
    InternalCompletion { shell: String },
    /// Get workspace directory
    #[command(hide = true)]
    GetSyncDirectory,
}

#[cfg(test)]
mod tests;
