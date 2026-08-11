use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum PackageInfrastructureRuntime {
    /// Reuse the last runtime; first setup uses Tart on macOS and Docker elsewhere
    Auto,
    /// Run the appliance directly in Docker
    Docker,
    /// Run Docker Compose inside a dedicated Linux Tart VM
    Tart,
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
    /// Create or update the shared package-infrastructure appliance
    Up {
        #[arg(long, value_enum, default_value = "auto")]
        runtime: PackageInfrastructureRuntime,
        #[arg(long, default_value = "3080")]
        port: u16,
        /// Override the immutable registry service image
        #[arg(long)]
        registry_image: Option<String>,
        /// Override the immutable package review/release job image
        #[arg(long, alias = "review-image")]
        job_image: Option<String>,
    },
    /// Stop the appliance while preserving all named volumes
    Down {
        #[arg(long, value_enum, default_value = "auto")]
        runtime: PackageInfrastructureRuntime,
    },
    /// Show the appliance runtime and gateway health
    Status {
        #[arg(long, value_enum, default_value = "auto")]
        runtime: PackageInfrastructureRuntime,
    },
    /// Validate the runtime, appliance definition, and gateway
    Doctor {
        #[arg(long, value_enum, default_value = "auto")]
        runtime: PackageInfrastructureRuntime,
    },
    /// List appliance-local infrastructure backups
    Backups {
        #[arg(long, value_enum, default_value = "auto")]
        runtime: PackageInfrastructureRuntime,
    },
    /// Create a consistent backup in a private named volume
    Backup {
        #[arg(long, value_enum, default_value = "auto")]
        runtime: PackageInfrastructureRuntime,
    },
    /// Restore a private named-volume backup while services are stopped
    Restore {
        backup_id: String,
        #[arg(long, value_enum, default_value = "auto")]
        runtime: PackageInfrastructureRuntime,
    },
    /// Register canonical repositories explicitly or discover them from local paths
    Register {
        /// One explicit package name, or one or more package repository paths
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
        /// CI-accessible registry endpoint used for synchronized releases
        #[arg(long)]
        ci_registry: Option<String>,
        /// Discover Git repositories below each supplied directory
        #[arg(long)]
        recursive: bool,
    },
    /// List registered shared-package repositories
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
    /// Create an isolated package checkout and attach it to this project
    Checkout {
        package: String,
        #[arg(long)]
        agent: String,
        /// Defaults to the current project
        #[arg(long)]
        consumer: Option<String>,
        #[arg(long)]
        task: String,
    },
    /// Show one package checkout
    Show { checkout_id: String },
    /// Cancel an eligible checkout and remove its temporary data
    Cancel { checkout_id: String },
    /// Remove a terminal checkout's temporary service and project data
    Cleanup { checkout_id: String },
    /// Validate and submit committed package work for integration review
    Submit {
        checkout_id: String,
        /// Defaults to the current project
        #[arg(long)]
        consumer: Option<String>,
    },
    /// Rebase or merge an approved submission and rerun selected checks
    Integrate {
        submission_id: String,
        /// Defaults to the current project
        #[arg(long)]
        consumer: Option<String>,
        #[arg(long, default_value = "rebase", value_parser = ["rebase", "merge"])]
        strategy: String,
    },
    /// Push validated source/tag and publish immutable release artifacts
    Publish {
        submission_id: String,
        /// Explicitly authorize the source commit and release tag push
        #[arg(long)]
        push_source: bool,
    },
    /// Create, test, and push one isolated consumer upgrade branch
    Rollout {
        /// Package and immutable version, for example auth@1.5.0
        target: String,
        #[arg(long = "to")]
        consumer: String,
    },
    /// Install or clear the controller's private Git token
    Auth {
        #[arg(
            long,
            alias = "git-token-file",
            conflicts_with_all = ["clear", "github"]
        )]
        token_file: Option<PathBuf>,
        /// Import the active GitHub CLI token without printing it
        #[arg(long, conflicts_with = "clear")]
        github: bool,
        #[arg(long, conflicts_with = "clear_ci")]
        ci_token_file: Option<PathBuf>,
        #[arg(long, conflicts_with = "github")]
        clear: bool,
        #[arg(long)]
        clear_ci: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ToolsSubcommand {
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
    /// List registered tool sources
    List,
    /// Show one registered tool and its published releases
    Show { name: String },
    /// Publish the current source of one registered collection
    Publish { name: String },
    /// Refresh the appliance-generated tool catalog cache
    Refresh {
        #[arg(long, hide = true)]
        quiet: bool,
    },
    /// Show tools active inside one environment
    Status { environment: Option<String> },
    /// Install configured tools and apply selected updates
    Update {
        environment: Option<String>,
        /// Select every available update without showing the checklist
        #[arg(long)]
        all: bool,
        /// Return after starting concurrent guest downloads
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
