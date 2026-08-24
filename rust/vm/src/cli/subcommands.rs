use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum PackageInfrastructureEngine {
    /// Reuse the configured appliance engine; first setup follows the container provider
    Auto,
    /// Run the appliance with Docker
    Docker,
    /// Run the appliance with Podman
    Podman,
}

#[derive(Debug, Clone, Subcommand)]
pub enum PackageConsumerSubcommand {
    /// Register a consumer repository and its current internal dependencies
    Register {
        name: String,
        #[arg(long)]
        repository: String,
        #[arg(long, default_value = "main")]
        branch: String,
        /// Repeat as --dependency package@version
        #[arg(long = "dependency", required = true)]
        dependencies: Vec<String>,
    },
    /// List registered consumer repositories
    List,
}

#[derive(Debug, Clone, Subcommand)]
pub enum PackagesSubcommand {
    /// Configure the controller source shelf and package appliance
    Init {
        #[arg(value_name = "SOURCE_ROOT")]
        source_root: PathBuf,
        #[arg(long, value_enum, default_value = "auto", hide = true)]
        engine: PackageInfrastructureEngine,
        /// Override the host gateway port (useful for isolated acceptance environments)
        #[arg(long, default_value = "3080")]
        port: u16,
        #[arg(long, hide = true)]
        registry_image: Option<String>,
        #[arg(long, hide = true)]
        job_image: Option<String>,
    },
    /// Prepare or reconcile the shared package-infrastructure appliance and configured sources
    Up {
        #[arg(long, value_enum, default_value = "auto")]
        engine: PackageInfrastructureEngine,
        #[arg(long, default_value = "3080")]
        port: u16,
        /// Override the immutable registry service image
        #[arg(long)]
        registry_image: Option<String>,
        /// Override the immutable package review/release job image
        #[arg(long)]
        job_image: Option<String>,
    },
    /// Stop the appliance while preserving all named volumes
    Down,
    /// Show the appliance engine and gateway health
    Status,
    /// Validate the runtime, appliance definition, and gateway
    Doctor {
        /// Apply safe, deterministic package-infrastructure repairs
        #[arg(long)]
        fix: bool,
    },
    /// List appliance-local infrastructure backups
    Backups,
    /// Create a consistent backup in a private named volume
    Backup,
    /// Restore a private named-volume backup while services are stopped
    Restore { backup_id: String },
    /// Register repository URLs or remember local Git roots as read-only workspaces
    Register {
        /// One explicit package name, or local Git roots remembered after registration
        #[arg(required = true, value_name = "NAME_OR_PATH")]
        targets: Vec<String>,
        #[arg(long, value_parser = ["npm", "cargo", "python"])]
        ecosystem: Option<String>,
        /// Canonical repository URL; omit to infer each path's origin remote
        #[arg(long)]
        repository: Option<String>,
        /// Override the inferred default branch
        #[arg(long)]
        branch: Option<String>,
        /// Discover Git repositories below each supplied directory
        #[arg(long)]
        recursive: bool,
    },
    /// List registered packages and their publication/consumability state
    List,
    /// Manage consumer repositories tracked by the package infrastructure
    Consumer {
        #[command(subcommand)]
        command: PackageConsumerSubcommand,
    },
    /// Show consumers and pending upgrades for one package
    Consumers { package: String },
    /// Show package-version drift across registered consumers
    Drift,
    /// Open an attested package or tool in its owning Docker workspace without copying it
    Open {
        #[arg(value_name = "SOURCE")]
        source: String,
    },
    /// Create or resume an isolated package or tool checkout in this managed guest
    Checkout {
        #[arg(value_name = "SOURCE")]
        source: String,
    },
    /// Show one managed source checkout (controller diagnostic)
    #[command(hide = true)]
    Show { checkout_id: String },
    /// Release the managed checkout or canonical workspace containing this directory
    Release,
    /// Cancel and clean up the managed checkout containing this directory
    Cancel,
    /// Install or clear the controller's private Git token
    Auth {
        #[arg(long, conflicts_with_all = ["clear", "github"])]
        token_file: Option<PathBuf>,
        /// Import the active GitHub CLI token without printing it
        #[arg(long, conflicts_with = "clear")]
        github: bool,
        #[arg(long, conflicts_with = "github")]
        clear: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ToolsSubcommand {
    #[command(hide = true)]
    ActivationWorker {
        #[arg(long)]
        once: bool,
    },
    /// Register one trusted tool source with package infrastructure
    Register {
        name: String,
        #[arg(long)]
        repository: String,
        #[arg(long, default_value = "main")]
        branch: String,
        #[arg(long, default_value = "binary", value_parser = ["binary", "collection"])]
        kind: String,
    },
    /// List registered tools and whether an artifact has been published
    List,
    /// Show one registered tool and its published releases
    Show { name: String },
    /// Refresh the appliance-generated tool catalog cache
    Refresh {
        #[arg(long, hide = true)]
        quiet: bool,
    },
    /// Show registered, published, installed, and consumable tool state
    Status { environment: Option<String> },
    /// Select tools globally and activate them in running managed Docker environments
    Enable {
        #[arg(required = true, value_name = "TOOL")]
        tools: Vec<String>,
    },
    /// Stop selecting tools globally; existing managed files are retained
    Disable {
        #[arg(required = true, value_name = "TOOL")]
        tools: Vec<String>,
    },
    /// Update configured tools across running managed environments
    Update {
        /// Tool names to filter; omit to update every configured tool
        #[arg(value_name = "TOOL")]
        tools: Vec<String>,
        /// Update only these managed environments
        #[arg(long, value_name = "ENVIRONMENT", action = clap::ArgAction::Append)]
        to: Vec<String>,
        /// Include stopped environments and start them in place
        #[arg(long)]
        include_stopped: bool,
        /// Reconcile prerequisites, then return after launching managed-tool downloads
        #[arg(long)]
        background: bool,
    },
}

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

#[derive(Debug, Clone, Default, clap::Args)]
pub struct FleetArgs {
    /// Apply the command across matching managed environments
    #[arg(long)]
    pub fleet: bool,
    /// Provider filter (docker, podman, tart)
    #[arg(
        long,
        requires = "fleet",
        value_parser = vm_config::config::ProviderName::SUPPORTED
    )]
    pub provider: Option<String>,
    /// Match pattern for instance names
    #[arg(long, requires = "fleet")]
    pub pattern: Option<String>,
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
