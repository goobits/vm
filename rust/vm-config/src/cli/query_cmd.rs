use crate::cli::{OutputFormat, TransformFormat};
use clap::Subcommand;
use std::path::PathBuf;

/// Commands for querying and filtering data.
#[derive(Subcommand)]
#[command(verbatim_doc_comment)]
pub enum QueryCmd {
    /// Query a specific field using dot notation
    Query {
        /// Config file
        config: PathBuf,

        /// Field path (e.g., "project.name" or "services.docker.enabled")
        field: String,

        /// Raw output (no quotes for strings)
        #[arg(short, long)]
        raw: bool,

        /// Default value if field is missing or null
        #[arg(short, long)]
        default: Option<String>,
    },

    /// Query with conditional filtering
    Filter {
        /// YAML file to query
        file: PathBuf,

        /// Filter expression
        expression: String,

        /// Output format
        #[arg(short = 'f', long, default_value = "yaml")]
        output_format: OutputFormat,
    },

    /// Count items in array or object
    Count {
        /// Config file
        file: PathBuf,

        /// Path to count (dot notation, empty for root)
        #[arg(default_value = "")]
        path: String,
    },

    /// Select items from array where field matches value
    SelectWhere {
        /// Config file
        file: PathBuf,

        /// Path to array (dot notation)
        path: String,

        /// Field name to match
        field: String,

        /// Value to match
        value: String,

        /// Output format
        #[arg(short = 'f', long, default_value = "yaml")]
        format: OutputFormat,
    },

    /// Check if field exists and has subfield
    HasField {
        /// Config file
        file: PathBuf,

        /// Field path to check
        field: String,

        /// Subfield to check for existence
        subfield: String,
    },
    /// Transform data with expressions
    Transform {
        /// Input file
        file: PathBuf,

        /// Transform expression
        expression: String,

        /// Output format
        #[arg(short = 'f', long, default_value = "lines")]
        format: TransformFormat,
    },
}