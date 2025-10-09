use clap::Subcommand;
use std::path::PathBuf;

/// Commands for managing configuration values.
#[derive(Subcommand)]
#[command(verbatim_doc_comment)]
pub enum ConfigCmd {
    /// Set a configuration value
    Set {
        /// Field path (e.g., "vm.memory" or "services.docker.enabled")
        field: String,
        /// Value to set
        value: String,
        /// Apply to global config (~/.config/vm/global.yaml)
        #[arg(long)]
        global: bool,
    },

    /// Get configuration value(s)
    Get {
        /// Field path (omit to show all configuration)
        field: Option<String>,
        /// Read from global config
        #[arg(long)]
        global: bool,
    },

    /// Remove a configuration field
    Unset {
        /// Field path to remove
        field: String,
        /// Remove from global config
        #[arg(long)]
        global: bool,
    },
    /// Validate a configuration file
    Validate {
        /// Config file to validate (optional, searches for vm.yaml if not provided)
        #[arg()]
        file: Option<PathBuf>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Load, merge, and validate configuration (outputs final YAML)
    Dump {
        /// Config file path (optional, searches for vm.yaml if not provided)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    /// Load, merge, and validate configuration, outputting as shell export commands.
    Export {
        /// Config file path (optional, searches for vm.yaml if not provided)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
}