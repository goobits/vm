use std::path::PathBuf;

use clap::Subcommand;

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigSubcommand {
    /// Validate the current configuration
    Validate,
    /// Show the loaded configuration and its source
    Show,
    /// Render the redacted provider configuration without applying it
    Render {
        /// Render a named instance instead of the default instance
        #[arg(long)]
        instance: Option<String>,
    },
    /// Change a configuration value
    Set {
        /// Configuration field path (e.g., "vm.memory" or "services.docker.enabled")
        field: String,
        /// Value(s) to set
        #[arg(required = true, num_args = 1..)]
        values: Vec<String>,
        /// Apply to global configuration
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
        /// Preset names (comma-separated for multiple)
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
    /// Manage configuration profiles
    Profile {
        #[command(subcommand)]
        command: ConfigProfileSubcommand,
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
pub enum ConfigProfileSubcommand {
    /// List available profiles for this project
    Ls,
    /// Set the default profile for this project
    Set { name: String },
}

#[derive(Debug, Clone, clap::Args)]
pub struct FleetTargetArgs {
    /// Provider filter (docker, podman, tart)
    #[arg(long)]
    pub provider: Option<String>,
    /// Match pattern for instance names
    #[arg(long)]
    pub pattern: Option<String>,
    /// Only include running instances
    #[arg(long)]
    pub running: bool,
    /// Only include stopped instances
    #[arg(long)]
    pub stopped: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum FleetSubcommand {
    /// Run a command across instances
    #[command(trailing_var_arg = true)]
    Exec {
        #[command(flatten)]
        targets: FleetTargetArgs,
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// Copy files to/from instances
    Copy {
        #[command(flatten)]
        targets: FleetTargetArgs,
        source: String,
        destination: String,
    },
    /// Start instances
    Start {
        #[command(flatten)]
        targets: FleetTargetArgs,
    },
    /// Stop instances
    Stop {
        #[command(flatten)]
        targets: FleetTargetArgs,
    },
    /// Restart instances
    Restart {
        #[command(flatten)]
        targets: FleetTargetArgs,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TunnelSubcommand {
    /// Add a tunnel (e.g., vm tunnel add 8080:3000 backend)
    Add {
        mapping: String,
        environment: Option<String>,
    },
    /// List active tunnels
    Ls { environment: Option<String> },
    /// Stop tunnel(s)
    Stop {
        port: Option<u16>,
        environment: Option<String>,
        #[arg(long)]
        all: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SecretSubcommand {
    /// Check secret proxy status
    Status,
    /// Store a secret
    Add {
        name: String,
        value: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// See all secrets
    Ls {
        #[arg(long)]
        show_values: bool,
    },
    /// Delete a secret
    Rm {
        name: String,
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// Add a secret interactively
    Interactive,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DbSubcommand {
    /// Backup a database
    Backup {
        db_name: Option<String>,
        name: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Restore a database from a backup
    Restore { name: String, db_name: String },
    /// List all databases and backups
    Ls,
    /// Export a database to a SQL file
    Export { name: String, file: PathBuf },
    /// Import a database from a SQL file
    Import { file: PathBuf, db_name: String },
    /// Show disk usage per database
    Size,
    /// Drop and recreate a database
    Reset {
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// Show credentials for a database service
    Credentials { service: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum RegistrySubcommand {
    /// Check registry server status
    Status {
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// List packages in the registry
    Ls {
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Manage registry settings
    Config {
        #[command(subcommand)]
        action: RegistryConfigAction,
    },
    /// Get shell configuration for using the registry
    Use {
        #[arg(long)]
        shell: Option<String>,
        #[arg(long, default_value = "3080")]
        port: u16,
    },
    /// Start registry server
    #[command(hide = true)]
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "3080")]
        port: u16,
        #[arg(long)]
        data: PathBuf,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum RegistryConfigAction {
    Show,
    Get { key: String },
    Set { key: String, value: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum BaseSubcommand {
    /// Build a provider-native base artifact for a preset
    Build {
        preset: String,
        #[arg(long, value_parser = ["docker", "tart"])]
        provider: String,
        /// Tart guest OS to build. Auto follows the active config/profile.
        #[arg(long = "guest-os", value_parser = ["auto", "linux", "macos"], default_value = "auto")]
        guest_os: String,
    },
    /// Validate the shared provider workflow for the current project
    Validate {
        preset: String,
        #[arg(long, value_parser = ["docker", "tart", "all"], default_value = "all")]
        provider: String,
        #[arg(long)]
        rebuild_docker_base: bool,
        #[arg(long)]
        build_tart_base: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SystemSubcommand {
    /// Update this vm installation
    Update {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Remove vm from this system
    Uninstall {
        #[arg(long)]
        keep_config: bool,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Manage package registries
    Registry {
        #[command(subcommand)]
        command: RegistrySubcommand,
    },
    /// Build and validate provider-native base environments
    Base {
        #[command(subcommand)]
        command: BaseSubcommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PluginSubcommand {
    /// See installed plugins
    Ls,
    /// Get plugin details
    Info { plugin_name: String },
    /// Add a plugin
    Install { source_path: String },
    /// Remove a plugin
    Rm { plugin_name: String },
    /// Create a new plugin
    New {
        plugin_name: String,
        #[arg(long)]
        r#type: String,
    },
    /// Check plugin configuration
    Validate { plugin_name: String },
}
