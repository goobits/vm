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
    /// Show client, controller, provider, and schema versions
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
    /// List VM-owned provider storage with deletion eligibility
    List {
        /// Emit one versioned JSON result
        #[arg(long)]
        json: bool,
    },
    /// Remove one exact, unreferenced disposable resource
    Remove {
        resource_id: String,
        #[arg(long)]
        yes: bool,
        /// Emit one versioned JSON result
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PluginSubcommand {
    /// See installed plugins
    List {
        #[arg(long)]
        json: bool,
    },
    /// Get plugin details
    Show {
        plugin_name: String,
        #[arg(long)]
        json: bool,
    },
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
