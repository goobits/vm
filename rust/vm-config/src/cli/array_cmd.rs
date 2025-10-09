use crate::cli::OutputFormat;
use clap::Subcommand;
use std::path::PathBuf;

/// Commands for array manipulation.
#[derive(Subcommand)]
#[command(verbatim_doc_comment)]
pub enum ArrayCmd {
    /// Add an item to a YAML array
    ArrayAdd {
        /// YAML file to modify
        file: PathBuf,

        /// Path to array (dot notation)
        path: String,

        /// YAML item to add (as string)
        item: String,
    },

    /// Remove items from a YAML array
    ArrayRemove {
        /// YAML file to modify
        file: PathBuf,

        /// Path to array (dot notation)
        path: String,

        /// Filter expression to match items to remove
        filter: String,
    },

    /// Get array length
    ArrayLength {
        /// Config file
        file: PathBuf,

        /// Path to array (dot notation, empty for root)
        #[arg(default_value = "")]
        path: String,
    },

    /// Add object to array
    AddToArray {
        /// YAML file to modify
        file: PathBuf,

        /// Path to array (dot notation)
        path: String,

        /// JSON object to add
        object: String,

        /// Output to stdout instead of modifying file
        #[arg(long)]
        stdout: bool,
    },

    /// Delete items from array matching a condition
    Delete {
        /// Config file
        file: PathBuf,

        /// Path to array (dot notation)
        path: String,

        /// Field to match for deletion
        field: String,

        /// Value to match for deletion
        value: String,

        /// Output format
        #[arg(short = 'f', long, default_value = "yaml")]
        format: OutputFormat,
    },
}
