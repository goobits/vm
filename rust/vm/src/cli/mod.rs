// CLI argument parsing and definitions

// Standard library imports
use std::path::PathBuf;

// External crate imports
use clap::{Parser, Subcommand};

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
pub enum PkgSubcommand {
    /// Show registry status and package counts
    Status,
    /// Add package from current directory
    Add {
        /// Specify package type(s) to publish (python,npm,cargo)
        #[arg(long, short = 't')]
        r#type: Option<String>,
    },
    /// Remove package from registry
    Remove {
        /// Skip confirmation prompts
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// List all packages in registry
    List,
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: PkgConfigAction,
    },
    /// Generate shell configuration for package managers
    Use {
        /// Shell type (bash, zsh, fish)
        #[arg(long)]
        shell: Option<String>,
        /// Package server port
        #[arg(long, default_value = "3080")]
        port: u16,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PkgConfigAction {
    /// Show all configuration values
    Show,
    /// Get a specific configuration value
    Get {
        /// Configuration key
        key: String,
    },
    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// New value
        value: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum AuthSubcommand {
    /// Show auth proxy status and secret counts
    Status,
    /// Add a secret
    Add {
        /// Secret name
        name: String,
        /// Secret value
        value: String,
        /// Secret scope (global, project:NAME, instance:NAME)
        #[arg(long)]
        scope: Option<String>,
        /// Optional description
        #[arg(long)]
        description: Option<String>,
    },
    /// List all secrets
    List {
        /// Show secret values (masked)
        #[arg(long)]
        show_values: bool,
    },
    /// Remove a secret
    Remove {
        /// Secret name
        name: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// Interactively add a secret
    Interactive,
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
    /// Run comprehensive health checks on VM environment
    #[command(about = "Check system dependencies, configuration, and service health")]
    Doctor,
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
        /// Instance name (defaults to 'dev' for multi-instance providers)
        #[arg(long)]
        instance: Option<String>,
    },
    /// Start a VM
    Start {
        /// Container name, ID, or project name to start
        #[arg()]
        container: Option<String>,
    },
    /// Stop a VM or force-kill a specific container
    Stop {
        /// Container name or ID to stop (if not provided, stops current project VM gracefully)
        container: Option<String>,
    },
    /// Restart a VM
    Restart {
        /// Container name, ID, or project name to restart
        #[arg()]
        container: Option<String>,
    },
    /// Re-run VM provisioning
    Provision {
        /// Container name, ID, or project name to provision
        #[arg()]
        container: Option<String>,
    },
    /// Destroy a VM and clean up resources
    Destroy {
        /// Container name, ID, or project name to destroy
        #[arg()]
        container: Option<String>,
        /// Force destruction without confirmation
        #[arg(long)]
        force: bool,
        /// Destroy all instances across all providers
        #[arg(long)]
        all: bool,
        /// Destroy all instances from specific provider
        #[arg(long)]
        provider: Option<String>,
        /// Match pattern for instance names (e.g., "*-dev")
        #[arg(long)]
        pattern: Option<String>,
    },

    /// List all VMs with status and resource usage
    List {
        /// Show instances from all providers (already default behavior)
        #[arg(long)]
        all_providers: bool,
        /// Filter by specific provider (docker, tart, vagrant)
        #[arg(long)]
        provider: Option<String>,
        /// Show detailed information
        #[arg(long)]
        verbose: bool,
    },
    /// Show VM status and health
    Status {
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
    },
    /// Connect to VM via SSH
    Ssh {
        /// Container name, ID, or project name to connect to
        #[arg()]
        container: Option<String>,
        /// Directory path to start shell in
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Execute commands inside VM
    Exec {
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
        /// Command to execute inside VM
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// View VM logs
    Logs {
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
    },

    /// Manage temporary VMs
    Temp {
        #[command(subcommand)]
        command: TempSubcommand,
    },

    /// Package registry management
    Pkg {
        #[command(subcommand)]
        command: PkgSubcommand,
    },

    /// Auth proxy management
    Auth {
        #[command(subcommand)]
        command: AuthSubcommand,
    },

    /// Get workspace directory
    #[command(hide = true)]
    GetSyncDirectory,
}
