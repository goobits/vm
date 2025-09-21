// CLI argument parsing and definitions

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(name = "vm")]
#[command(about = "A modern, fast, and portable VM management tool")]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    /// Path to a custom VM configuration file
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Show what would be executed without running
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Enable debug output
    #[arg(short, long, global = true)]
    pub debug: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigSubcommand {
    /// Set configuration value
    Set {
        /// Configuration field path (e.g., "vm.memory" or "services.docker.enabled")
        field: String,
        /// Value to set
        value: String,
        /// Apply to global configuration (~/.config/vm/global.yaml)
        #[arg(long)]
        global: bool,
    },
    /// Get configuration values
    Get {
        /// Configuration field path (omit to show all)
        field: Option<String>,
        /// Read from global configuration
        #[arg(long)]
        global: bool,
    },
    /// Remove configuration field
    Unset {
        /// Configuration field path to remove
        field: String,
        /// Remove from global configuration
        #[arg(long)]
        global: bool,
    },
    /// Apply configuration presets
    Preset {
        /// Preset names (comma-separated for multiple, e.g., "nodejs,docker")
        names: Option<String>,
        /// Apply to global configuration
        #[arg(long)]
        global: bool,
        /// List available presets
        #[arg(long)]
        list: bool,
        /// Show preset details
        #[arg(long)]
        show: Option<String>,
    },
    /// Manage port configuration and resolve conflicts
    Ports {
        /// Fix port conflicts automatically
        #[arg(long)]
        fix: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TempSubcommand {
    /// Create temporary VM with mounts
    Create {
        /// Directories to mount (e.g., ./src,./config:ro)
        mounts: Vec<String>,

        /// Automatically destroy VM on exit
        #[arg(long)]
        auto_destroy: bool,
    },
    /// Connect to temporary VM via SSH
    Ssh,
    /// Show temporary VM status
    Status,
    /// Destroy temporary VM
    Destroy,
    /// Add mount to running temporary VM
    Mount {
        /// Path to mount (e.g., ./src or ./config:ro)
        path: String,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// Remove mount from temporary VM
    Unmount {
        /// Path to unmount (omit for --all)
        path: Option<String>,
        /// Remove all mounts
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// List current mounts
    Mounts,
    /// List all temporary VMs
    List,
    /// Stop temporary VM
    Stop,
    /// Start temporary VM
    Start,
    /// Restart temporary VM
    Restart,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Initialize a new VM configuration file
    Init {
        /// Custom VM configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Services to enable (comma-separated: postgresql,redis,mongodb,docker)
        #[arg(long)]
        services: Option<String>,

        /// Starting port for service allocation (allocates sequential ports)
        #[arg(long)]
        ports: Option<u16>,
    },
    /// Validate VM configuration
    Validate,
    /// Manage VM configuration settings
    Config {
        #[command(subcommand)]
        command: ConfigSubcommand,
    },

    /// Create and provision a new VM
    Create {
        /// Force creation even if VM already exists
        #[arg(long)]
        force: bool,
    },
    /// Start a VM
    Start,
    /// Stop a VM or force-kill a specific container
    Stop {
        /// Container name or ID to stop (if not provided, stops current project VM gracefully)
        container: Option<String>,
    },
    /// Restart a VM
    Restart,
    /// Re-run VM provisioning
    Provision,
    /// Destroy a VM and clean up resources
    Destroy {
        /// Force destruction without confirmation
        #[arg(long)]
        force: bool,
    },

    /// List all VMs with status and resource usage
    List,
    /// Show VM status and health
    Status,
    /// Connect to VM via SSH
    Ssh {
        /// Directory path to start shell in
        #[arg()]
        path: Option<PathBuf>,
    },
    /// Execute commands inside VM
    Exec {
        /// Command to execute inside VM
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// View VM logs
    Logs,

    /// Manage temporary VMs
    Temp {
        #[command(subcommand)]
        command: TempSubcommand,
    },

    /// Get workspace directory
    #[command(hide = true)]
    GetSyncDirectory,
}
