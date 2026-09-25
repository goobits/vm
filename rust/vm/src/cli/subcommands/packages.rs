use clap::{Subcommand, ValueEnum};
use std::path::PathBuf;

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
    /// Remove a consumer registration while preserving rollout records and source repositories
    Remove { name: String },
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
    Create { name: Option<String> },
    /// Remove one exact appliance-local backup
    Remove {
        name: String,
        #[arg(long)]
        yes: bool,
    },
    /// Restore a private named-volume backup while services are stopped
    Restore {
        name: String,
        #[arg(long)]
        yes: bool,
    },
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
    /// Remove a package registration while preserving published versions and source repositories
    Remove { name: String },
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
    Release {
        /// Observe or resume the checkout associated with a durable workflow receipt
        #[arg(long)]
        receipt: Option<String>,
        /// Return once the service has accepted the release submission
        #[arg(long)]
        background: bool,
    },
    /// Cancel and clean up the managed checkout containing this directory
    Cancel,
    /// Manage the controller's private Git token
    Auth {
        #[command(subcommand)]
        command: PackageAuthSubcommand,
    },
}
