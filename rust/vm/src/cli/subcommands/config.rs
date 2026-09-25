use clap::Subcommand;

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigSubcommand {
    /// Validate the current configuration
    Validate,
    /// Show the loaded configuration and its source
    Show {
        #[arg(long, value_enum, default_value_t = ConfigReadScope::Effective)]
        scope: ConfigReadScope,
        /// Emit one versioned JSON result
        #[arg(long)]
        json: bool,
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
        /// Emit one versioned JSON result
        #[arg(long)]
        json: bool,
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
    /// List available presets
    List,
    /// Show a preset and its configuration
    Show { name: String },
    /// Apply one or more presets to configuration
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
