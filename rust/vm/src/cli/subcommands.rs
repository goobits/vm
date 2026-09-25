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
    /// Retry failed dependency updates for a registered consumer
    Retry { name: String },
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
    List {
        #[arg(long)]
        package: Option<String>,
    },
    /// Show a registered consumer and its declared dependencies
    Show { name: String },
    /// Show package-version drift across registered consumers
    Drift {
        #[arg(long)]
        package: Option<String>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PackageBackupSubcommand {
    /// List appliance-local backups
    List,
    /// Create a consistent backup in a private named volume
    Create,
    /// Restore a private named-volume backup while services are stopped
    Restore { name: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PackageServiceSubcommand {
    /// Configure the controller source shelf and package appliance
    Init {
        #[arg(value_name = "SOURCE_ROOT", long)]
        source_root: PathBuf,
        #[arg(long, value_enum, default_value = "auto", hide = true)]
        engine: PackageInfrastructureEngine,
        #[arg(long, default_value = "3080")]
        port: u16,
        #[arg(long, hide = true)]
        registry_image: Option<String>,
        #[arg(long, hide = true)]
        job_image: Option<String>,
    },
    /// Show appliance engine and gateway health
    Status,
    /// Validate the runtime, appliance definition, and gateway
    Doctor {
        #[arg(long)]
        fix: bool,
    },
    /// Manage private appliance backups
    Backups {
        #[command(subcommand)]
        command: PackageBackupSubcommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PackageAuthSubcommand {
    /// Import an active GitHub credential or read a token from stdin or a file
    Login {
        #[arg(long, conflicts_with = "token_file")]
        token_stdin: bool,
        #[arg(long, conflicts_with = "token_stdin")]
        token_file: Option<PathBuf>,
    },
    /// Report whether a controller Git credential is configured
    Status,
    /// Remove the controller Git credential
    Logout,
}

#[derive(Debug, Clone, Subcommand)]
pub enum SnapshotSubcommand {
    /// List snapshots for the selected environment
    List {
        #[arg(long)]
        env: Option<String>,
    },
    /// Show snapshot metadata
    Show {
        name: String,
        #[arg(long)]
        env: Option<String>,
    },
    /// Capture an environment
    Create {
        name: String,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        quiesce: bool,
    },
    /// Restore a snapshot into an environment
    Restore {
        name: String,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Delete a snapshot
    Remove {
        name: String,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Export a snapshot as a portable archive
    Export {
        name: String,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 6)]
        compression: u8,
        /// Replace an existing archive
        #[arg(long)]
        overwrite: bool,
    },
    /// Import a portable snapshot archive
    Import {
        archive: PathBuf,
        #[arg(long)]
        name: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PackagesSubcommand {
    /// Prepare or reconcile the shared package-infrastructure appliance and configured sources
    Up {
        #[arg(long, value_enum, default_value = "auto")]
        engine: PackageInfrastructureEngine,
        /// Override the configured host gateway port
        #[arg(long)]
        port: Option<u16>,
        /// Override the immutable registry service image
        #[arg(long)]
        registry_image: Option<String>,
        /// Override the immutable package review/release job image
        #[arg(long)]
        job_image: Option<String>,
    },
    /// Stop the appliance while preserving all named volumes
    Down,
    /// Inspect and administer the package appliance
    Service {
        #[command(subcommand)]
        command: PackageServiceSubcommand,
    },
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
    /// Show one registered package
    Show { name: String },
    /// Manage consumer repositories tracked by the package infrastructure
    Consumers {
        #[command(subcommand)]
        command: PackageConsumerSubcommand,
    },
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
    CheckoutShow { checkout_id: String },
    /// Release the managed checkout or canonical workspace containing this directory
    Release,
    /// Cancel and clean up the managed checkout containing this directory
    Cancel,
    /// Manage the controller's private Git token
    Auth {
        #[command(subcommand)]
        command: PackageAuthSubcommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ToolsSubcommand {
    #[command(hide = true)]
    ActivationWorker {
        #[arg(long)]
        once: bool,
    },
    #[command(hide = true)]
    ReconcileWorker { environment: String },
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
    /// List VM-owned vendor tools and registered package tools
    List,
    /// Show one vendor definition or package tool and its published releases
    Show { name: String },
    /// Refresh the appliance-generated tool catalog cache
    Refresh {
        #[arg(long, hide = true)]
        quiet: bool,
    },
    /// Show vendor, registered, published, installed, and consumable tool state
    Status {
        #[arg(long)]
        env: Option<String>,
    },
    /// Select package tools globally and activate them in running managed Docker environments
    Enable {
        #[arg(required = true, value_name = "TOOL")]
        tools: Vec<String>,
    },
    /// Stop selecting package tools globally; existing managed files are retained
    Disable {
        #[arg(required = true, value_name = "TOOL")]
        tools: Vec<String>,
    },
    /// Update VM-owned vendor tools and configured package tools across managed environments
    Update {
        /// Vendor or package tool names to filter; omit to update all eligible tools
        #[arg(value_name = "TOOL")]
        tools: Vec<String>,
        /// Update only these project environments
        #[arg(long, value_name = "ENVIRONMENT", action = clap::ArgAction::Append, conflicts_with = "all_envs")]
        env: Vec<String>,
        /// Update every running environment in this project
        #[arg(long, conflicts_with = "env")]
        all_envs: bool,
        /// Record updates for stopped environments without starting them
        #[arg(long)]
        include_stopped: bool,
        /// Reconcile prerequisites, then return after launching tool updates
        #[arg(long)]
        background: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigSubcommand {
    /// Validate the current configuration
    Validate,
    /// Show the loaded configuration and its source
    Show {
        #[arg(long, value_enum, default_value_t = ConfigReadScope::Effective)]
        scope: ConfigReadScope,
    },
    /// Render the redacted provider configuration without applying it
    Render {
        /// Render a named environment instead of the configured default
        #[arg(long = "env")]
        env: Option<String>,
    },
    /// Change a configuration value
    Set {
        /// Configuration field path (e.g., "vm.memory" or "services.docker.enabled")
        field: String,
        /// Scalar value(s) to set
        #[arg(num_args = 1.., required_unless_present = "value_json", conflicts_with = "value_json")]
        values: Vec<String>,
        /// JSON array or object value
        #[arg(long, conflicts_with = "values")]
        value_json: Option<String>,
        #[arg(long, value_enum, default_value_t = ConfigWriteScope::Project)]
        scope: ConfigWriteScope,
    },
    /// View configuration values
    Get {
        /// Configuration field path (omit to show all)
        field: String,
        #[arg(long, value_enum, default_value_t = ConfigReadScope::Effective)]
        scope: ConfigReadScope,
    },
    /// Remove a configuration value
    Unset {
        /// Configuration field path to remove
        field: String,
        #[arg(long, value_enum, default_value_t = ConfigWriteScope::Project)]
        scope: ConfigWriteScope,
    },
    /// Manage configuration presets
    Presets {
        #[command(subcommand)]
        command: ConfigPresetSubcommand,
    },
    /// Manage configuration profiles
    Profiles {
        #[command(subcommand)]
        command: ConfigProfileSubcommand,
    },
    /// Fix port conflicts
    Ports {
        /// Fix port conflicts automatically
        #[arg(long)]
        fix: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigProfileSubcommand {
    /// List available profiles for this project
    List,
    /// Show a named profile
    Show { name: String },
    /// Set the default profile for this project
    SetDefault { name: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigPresetSubcommand {
    List,
    Show {
        name: String,
    },
    Apply {
        #[arg(required = true, num_args = 1..)]
        names: Vec<String>,
        #[arg(long, value_enum, default_value_t = ConfigWriteScope::Project)]
        scope: ConfigWriteScope,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ConfigReadScope {
    Project,
    User,
    Effective,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ConfigWriteScope {
    Project,
    User,
}

#[derive(Debug, Clone, Default, clap::Args)]
pub struct FleetArgs {
    /// Apply the command across matching managed environments
    #[arg(long = "all-envs")]
    pub fleet: bool,
    /// Provider filter (docker, podman, tart)
    #[arg(
        long = "match-provider",
        requires = "fleet",
        value_parser = vm_config::config::ProviderName::SUPPORTED
    )]
    pub provider: Option<String>,
    /// Match pattern for instance names
    #[arg(long = "match", requires = "fleet")]
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum TunnelSubcommand {
    /// Open a named loopback tunnel to a port in an environment
    Open {
        name: String,
        #[arg(long)]
        local: String,
        #[arg(long)]
        remote: String,
        #[arg(long)]
        env: Option<String>,
    },
    /// List active tunnels
    List {
        #[arg(long)]
        env: Option<String>,
    },
    /// Close one named tunnel
    Close {
        name: String,
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SecretSubcommand {
    /// Check secret proxy status
    Status,
    /// Store a secret
    Set {
        name: String,
        #[arg(long, conflicts_with = "file")]
        stdin: bool,
        #[arg(long, conflicts_with = "stdin")]
        file: Option<PathBuf>,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// See all secrets
    List,
    /// Reveal one secret value
    Show {
        name: String,
        #[arg(long, required = true)]
        reveal: bool,
    },
    /// Delete a secret
    Remove {
        name: String,
        #[arg(long, short = 'f')]
        force: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DbSubcommand {
    /// Manage PostgreSQL backups
    Backups {
        #[command(subcommand)]
        command: DbBackupSubcommand,
    },
    /// List databases
    List,
    /// Show the size and backup count of a database
    Status { name: String },
    /// Export a database to a SQL file
    Export {
        name: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Import a database from a SQL file
    Import {
        name: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        yes: bool,
    },
    /// Drop and recreate a database
    Reset {
        name: String,
        #[arg(long)]
        yes: bool,
    },
    /// Show credentials metadata, or reveal the value explicitly
    Credentials {
        service: String,
        #[arg(long)]
        reveal: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DbBackupSubcommand {
    /// List retained backups
    List {
        #[arg(long)]
        database: Option<String>,
    },
    /// Create a backup for one database or all databases
    Create {
        name: String,
        #[arg(long, conflicts_with = "all", required_unless_present = "all")]
        database: Option<String>,
        #[arg(long, conflicts_with = "database")]
        all: bool,
    },
    /// Restore a backup into one database
    Restore {
        backup: String,
        #[arg(long)]
        database: String,
        #[arg(long)]
        yes: bool,
    },
    /// Remove one retained backup
    Remove {
        backup: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum BaseSubcommand {
    /// Build a provider-native base artifact for a preset
    Build {
        preset: String,
        #[arg(long, value_parser = ["docker", "podman", "tart"])]
        provider: String,
        /// Tart guest OS to build. Auto follows the active config/profile.
        #[arg(long = "guest-os", value_parser = ["auto", "linux", "macos"], default_value = "auto")]
        guest_os: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SystemSubcommand {
    /// Show this installation's version and location
    Info,
    /// Update this vm installation
    Update {
        #[arg(long)]
        version: Option<String>,
    },
    /// Remove vm from this system
    Uninstall {
        #[arg(long)]
        delete_config: bool,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Manage provider-native base images
    Images {
        #[command(subcommand)]
        command: BaseSubcommand,
    },
    /// Inspect and remove VM-owned provider storage
    Storage {
        #[command(subcommand)]
        command: SystemStorageSubcommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SystemStorageSubcommand {
    /// List VM-owned volumes and images with deletion eligibility
    List,
    /// Remove one exact, unreferenced disposable resource
    Remove {
        resource_id: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PluginSubcommand {
    /// See installed plugins
    List,
    /// Get plugin details
    Show { plugin_name: String },
    /// Add a plugin
    Install { source_path: String },
    /// Remove a plugin
    Remove { plugin_name: String },
    /// Create a new plugin
    Create {
        plugin_name: String,
        #[arg(long, value_parser = ["preset", "service"], ignore_case = true)]
        kind: String,
    },
    /// Check plugin configuration
    Validate { plugin_name: String },
}
