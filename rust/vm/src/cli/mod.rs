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

    /// Path to a custom vm.yaml configuration file
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
    /// Set a configuration value
    Set {
        /// Field path (e.g., "vm.memory" or "services.docker.enabled")
        field: String,
        /// Value to set
        value: String,
        /// Apply to global config (~/.config/vm/global.yaml)
        #[arg(long)]
        global: bool,
    },
    /// Get configuration value(s)
    Get {
        /// Field path (omit to show all configuration)
        field: Option<String>,
        /// Read from global config
        #[arg(long)]
        global: bool,
    },
    /// Remove a configuration field
    Unset {
        /// Field path to remove
        field: String,
        /// Remove from global config
        #[arg(long)]
        global: bool,
    },
    /// Apply preset(s) to configuration
    Preset {
        /// Preset name(s), comma-separated for multiple (e.g., "nodejs,docker")
        names: Option<String>,
        /// Apply to global config
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
    /// Create temp VM with mounts
    Create {
        /// Directories to mount (e.g., ./src,./config:ro)
        mounts: Vec<String>,

        /// Auto-destroy on exit
        #[arg(long)]
        auto_destroy: bool,
    },
    /// SSH into temp VM
    Ssh,
    /// Show temp VM status
    Status,
    /// Destroy temp VM
    Destroy,
    /// Add mount to running temp VM
    Mount {
        /// Path to mount (e.g., ./src or ./config:ro)
        path: String,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// Remove mount from temp VM
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
    /// List all temp VMs
    List,
    /// Stop temp VM
    Stop,
    /// Start temp VM
    Start,
    /// Restart temp VM
    Restart,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    // Configuration & Setup
    #[command(next_help_heading = "Configuration & Setup")]
    /// Initialize a new vm.yaml configuration file
    Init {
        /// Custom configuration file path
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Comma-separated services to enable (postgresql,redis,mongodb,docker)
        #[arg(long)]
        services: Option<String>,

        /// Starting port for service allocation (allocates sequential ports)
        #[arg(long)]
        ports: Option<u16>,
    },
    #[command(next_help_heading = "Configuration & Setup")]
    /// Validate the configuration
    Validate,
    #[command(next_help_heading = "Configuration & Setup")]
    /// Manage configuration settings (basic operations - use 'vm-config' tool for advanced features)
    Config {
        #[command(subcommand)]
        command: ConfigSubcommand,
    },

    // VM Lifecycle
    #[command(next_help_heading = "VM Lifecycle")]
    /// Create and provision a new VM
    Create {
        /// Force creation even if VM already exists
        #[arg(long)]
        force: bool,
    },
    #[command(next_help_heading = "VM Lifecycle")]
    /// Start an existing VM
    Start,
    #[command(next_help_heading = "VM Lifecycle")]
    /// Stop a running VM or force kill a specific container
    Stop {
        /// Optional container name or ID to stop. If not provided, stops the current project's VM gracefully.
        container: Option<String>,
    },
    #[command(next_help_heading = "VM Lifecycle")]
    /// Restart a VM (stop then start)
    Restart,
    #[command(next_help_heading = "VM Lifecycle")]
    /// Destroy a VM and its resources
    Destroy {
        /// Force destruction without confirmation
        #[arg(long)]
        force: bool,
    },
    #[command(next_help_heading = "VM Lifecycle")]
    /// Re-run provisioning on existing VM
    Provision,

    // VM Operations
    #[command(next_help_heading = "VM Operations")]
    /// SSH into a VM
    Ssh {
        /// Optional path to start the shell in
        #[arg()]
        path: Option<PathBuf>,
    },
    #[command(next_help_heading = "VM Operations")]
    /// Execute a command in the VM
    Exec {
        /// The command to execute
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },

    // Monitoring & Information
    #[command(next_help_heading = "Monitoring & Information")]
    /// List all VMs
    List,
    #[command(next_help_heading = "Monitoring & Information")]
    /// Get the status of a VM
    Status,
    #[command(next_help_heading = "Monitoring & Information")]
    /// View logs of the VM
    Logs,

    // Temporary VMs
    #[command(next_help_heading = "Temporary VMs")]
    /// Temporary VM operations
    Temp {
        #[command(subcommand)]
        command: TempSubcommand,
    },

    /// Get workspace directory
    #[command(hide = true)]
    GetSyncDirectory,
}
