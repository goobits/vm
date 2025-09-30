// CLI argument parsing and definitions

// Standard library imports
use std::path::PathBuf;

// External crate imports
use clap::{Parser, Subcommand};

#[derive(Debug, Clone, Parser)]
#[command(name = "vm")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "Goobits VM Contributors")]
#[command(about = "Smart development environments for modern projects")]
#[command(before_help = format!(" \nvm v{}", env!("CARGO_PKG_VERSION")))]
#[command(after_help = " \n")]
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
    /// Change a configuration value
    Set {
        /// Configuration field path (e.g., "vm.memory" or "services.docker.enabled")
        field: String,
        /// Value to set
        value: String,
        /// Apply to global configuration (~/.config/vm/global.yaml)
        #[arg(long)]
        global: bool,
    },
    /// View configuration values
    Get {
        /// Configuration field path (omit to show all)
        field: Option<String>,
        /// Read from global configuration
        #[arg(long)]
        global: bool,
    },
    /// Remove a configuration value
    Unset {
        /// Configuration field path to remove
        field: String,
        /// Remove from global configuration
        #[arg(long)]
        global: bool,
    },
    /// Add preset configurations
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
    /// Fix port conflicts
    Ports {
        /// Fix port conflicts automatically
        #[arg(long)]
        fix: bool,
    },
    /// Reset your configuration
    Clear {
        /// Clear global configuration instead of local
        #[arg(long)]
        global: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TempSubcommand {
    /// Create a temporary environment
    Create {
        /// Directories to mount (e.g., ./src,./config:ro)
        mounts: Vec<String>,

        /// Automatically destroy VM on exit
        #[arg(long)]
        auto_destroy: bool,
    },
    /// Connect to your temp environment
    Ssh,
    /// Check temp environment status
    Status,
    /// Delete your temp environment
    Destroy,
    /// Add a folder to your temp environment
    Mount {
        /// Path to mount (e.g., ./src or ./config:ro)
        path: String,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// Remove a folder from your temp environment
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
    /// See mounted folders
    Mounts,
    /// See all temp environments
    List,
    /// Stop your temp environment
    Stop,
    /// Start your temp environment
    Start,
    /// Restart your temp environment
    Restart,
}

#[derive(Debug, Clone, Subcommand)]
pub enum PkgSubcommand {
    /// Check registry status
    Status,
    /// Publish a package
    Add {
        /// Specify package type(s) to publish (python,npm,cargo)
        #[arg(long, short = 't')]
        r#type: Option<String>,
        /// Start server automatically without prompting
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Remove a package
    Remove {
        /// Skip confirmation prompts
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// See all packages
    List,
    /// Manage registry settings
    Config {
        #[command(subcommand)]
        action: PkgConfigAction,
    },
    /// Get shell configuration
    Use {
        /// Shell type (bash, zsh, fish)
        #[arg(long)]
        shell: Option<String>,
        /// Package server port
        #[arg(long, default_value = "3080")]
        port: u16,
    },
    /// Start package server (internal use - for background process)
    #[command(hide = true)]
    Serve {
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        /// Port to bind to
        #[arg(long, default_value = "3080")]
        port: u16,
        /// Data directory for package storage
        #[arg(long)]
        data: std::path::PathBuf,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PkgConfigAction {
    /// View all settings
    Show,
    /// Get a specific setting
    Get {
        /// Configuration key
        key: String,
    },
    /// Change a setting
    Set {
        /// Configuration key
        key: String,
        /// New value
        value: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum AuthSubcommand {
    /// Check auth proxy status
    Status,
    /// Store a secret
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
    /// See all secrets
    List {
        /// Show secret values (masked)
        #[arg(long)]
        show_values: bool,
    },
    /// Delete a secret
    Remove {
        /// Secret name
        name: String,
        /// Skip confirmation prompt
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// Add a secret interactively
    Interactive,
}

#[derive(Debug, Clone, Subcommand)]
pub enum PluginSubcommand {
    /// See installed plugins
    List,
    /// Get plugin details
    Info {
        /// Plugin name
        plugin_name: String,
    },
    /// Add a plugin
    Install {
        /// Path to plugin directory
        source_path: String,
    },
    /// Remove a plugin
    Remove {
        /// Plugin name to remove
        plugin_name: String,
    },
    /// Create a new plugin
    New {
        /// Plugin name
        plugin_name: String,
        /// Plugin type (preset or service)
        #[arg(long)]
        r#type: String,
    },
    /// Check plugin configuration
    Validate {
        /// Plugin name to validate
        plugin_name: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Create a new configuration file
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
    /// Check your configuration for errors
    Validate,
    /// Run health checks and diagnostics
    #[command(about = "Check system dependencies, configuration, and service health")]
    Doctor,
    /// Update configuration settings
    Config {
        #[command(subcommand)]
        command: ConfigSubcommand,
    },

    /// Spin up a new development environment
    Create {
        /// Force creation even if VM already exists
        #[arg(long)]
        force: bool,
        /// Instance name (defaults to 'dev' for multi-instance providers)
        #[arg(long)]
        instance: Option<String>,
        /// Show detailed output including all Ansible tasks
        #[arg(long)]
        verbose: bool,
    },
    /// Start your environment
    Start {
        /// Container name, ID, or project name to start
        #[arg()]
        container: Option<String>,
    },
    /// Stop your environment
    Stop {
        /// Container name or ID to stop (if not provided, stops current project VM gracefully)
        container: Option<String>,
    },
    /// Restart your environment
    Restart {
        /// Container name, ID, or project name to restart
        #[arg()]
        container: Option<String>,
    },
    /// Reconfigure your environment
    Provision {
        /// Container name, ID, or project name to provision
        #[arg()]
        container: Option<String>,
    },
    /// Delete an environment
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

    /// See all your environments
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
    /// Check environment status
    Status {
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
    },
    /// Jump into your environment
    Ssh {
        /// Container name, ID, or project name to connect to
        #[arg()]
        container: Option<String>,
        /// Directory path to start shell in
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Run a command in your environment
    Exec {
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
        /// Command to execute inside VM
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// View environment logs
    Logs {
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
    },

    /// Work with temporary environments
    Temp {
        #[command(subcommand)]
        command: TempSubcommand,
    },

    /// Manage package registries
    Pkg {
        #[command(subcommand)]
        command: PkgSubcommand,
    },

    /// Manage secrets and credentials
    Auth {
        #[command(subcommand)]
        command: AuthSubcommand,
    },

    /// Extend with plugins
    Plugin {
        #[command(subcommand)]
        command: PluginSubcommand,
    },

    /// Update to the latest version
    Update {
        /// Specific version to install (e.g., v1.2.3)
        #[arg(long)]
        version: Option<String>,
        /// Force update even if already at latest version
        #[arg(long)]
        force: bool,
    },
    /// Remove from your system
    Uninstall {
        /// Keep configuration files
        #[arg(long)]
        keep_config: bool,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Get workspace directory
    #[command(hide = true)]
    GetSyncDirectory,
}
