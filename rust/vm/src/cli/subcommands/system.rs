use clap::Subcommand;

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
    Info {
        /// Emit one versioned JSON result
        #[arg(long)]
        json: bool,
    },
    /// Update this vm installation
    Update {
        #[arg(long)]
        version: Option<String>,
    },
    /// Remove vm from this system
    Uninstall {
        #[arg(long)]
        delete_config: bool,
        /// Delete VM-owned local state, snapshots, and cache
        #[arg(long)]
        delete_data: bool,
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
