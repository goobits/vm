// Removed unused imports
use anyhow::Result;
use clap::{Parser, Subcommand};
// Removed unused import
use std::path::PathBuf;
// Removed unused imports

mod commands;
mod utils;

pub use utils::*;
pub use commands::validation::{load_and_merge_config, load_and_merge_config_with_preset};

// Removed unused function fix_yaml_indentation

#[derive(Parser)]
#[command(name = "vm-config")]
#[command(about = "Configuration processor for VM Tool")]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Merge multiple config files
    Merge {
        /// Base config file
        #[arg(short, long)]
        base: PathBuf,

        /// Overlay config files (can specify multiple)
        #[arg(short, long)]
        overlay: Vec<PathBuf>,

        /// Output format
        #[arg(short = 'f', long, default_value = "yaml")]
        format: OutputFormat,
    },

    /// Validate a configuration file
    Validate {
        /// Config file to validate (optional, searches for vm.yaml if not provided)
        #[arg()]
        file: Option<PathBuf>,

        /// Disable automatic preset detection
        #[arg(long)]
        no_preset: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Detect and apply preset
    Preset {
        /// Project directory
        #[arg(short, long, default_value = ".")]
        dir: PathBuf,

        /// Presets directory (defaults to configs/presets relative to tool dir)
        #[arg(long)]
        presets_dir: Option<PathBuf>,

        /// Just detect, don't apply
        #[arg(long)]
        detect_only: bool,

        /// List available presets
        #[arg(short, long)]
        list: bool,
    },

    /// Convert between formats
    Convert {
        /// Input file
        input: PathBuf,

        /// Output format
        #[arg(short = 'f', long, default_value = "json")]
        format: OutputFormat,
    },

    /// Process config with full VM Tool logic (merge defaults, presets, user config)
    Process {
        /// Default config (defaults to vm.yaml in tool directory)
        #[arg(short, long)]
        defaults: Option<PathBuf>,

        /// User config (optional)
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Project directory for preset detection
        #[arg(short, long, default_value = ".")]
        project_dir: PathBuf,

        /// Presets directory (defaults to configs/presets relative to tool dir)
        #[arg(long)]
        presets_dir: Option<PathBuf>,

        /// Output format
        #[arg(short = 'f', long, default_value = "yaml")]
        format: OutputFormat,
    },

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

    /// Check if file is valid YAML
    CheckFile {
        /// YAML file to check
        file: PathBuf,
    },

    /// Merge multiple configuration files with deep merging
    MergeEvalAll {
        /// Files to merge
        files: Vec<PathBuf>,

        /// Output format
        #[arg(short = 'f', long, default_value = "yaml")]
        format: OutputFormat,
    },

    /// Modify YAML file in-place
    Modify {
        /// YAML file to modify
        file: PathBuf,

        /// Field path to set (dot notation)
        field: String,

        /// New value
        value: String,

        /// Output to stdout instead of modifying file
        #[arg(long)]
        stdout: bool,
    },

    /// Get array length
    ArrayLength {
        /// Config file
        file: PathBuf,

        /// Path to array (dot notation, empty for root)
        #[arg(default_value = "")]
        path: String,
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

    /// Check if field exists and has subfield
    HasField {
        /// Config file
        file: PathBuf,

        /// Field path to check
        field: String,

        /// Subfield to check for existence
        subfield: String,
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

    /// Count items in array or object
    Count {
        /// Config file
        file: PathBuf,

        /// Path to count (dot notation, empty for root)
        #[arg(default_value = "")]
        path: String,
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

    /// Load, merge, and validate configuration (outputs final YAML)
    Dump {
        /// Config file path (optional, searches for vm.yaml if not provided)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Disable automatic preset detection
        #[arg(long)]
        no_preset: bool,
    },

    /// Load, merge, and validate configuration, outputting as shell export commands.
    Export {
        /// Config file path (optional, searches for vm.yaml if not provided)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Disable automatic preset detection
        #[arg(long)]
        no_preset: bool,
    },

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

    /// Clear all configuration
    Clear {
        /// Clear global config
        #[arg(long)]
        global: bool,
    },

    /// Initialize a new vm.yaml configuration file
    Init {
        /// Target file or directory (defaults to current directory)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Comma-separated services to enable (postgresql,redis,mongodb,docker)
        #[arg(long)]
        services: Option<String>,

        /// Starting port for service allocation (allocates sequential ports)
        #[arg(long)]
        ports: Option<u16>,
    },
}

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Yaml,
    Json,
    JsonPretty,
}

#[derive(Clone, Debug)]
pub enum TransformFormat {
    Lines,
    Space,
    Comma,
    Json,
    Yaml,
}

impl std::str::FromStr for TransformFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "lines" => Ok(TransformFormat::Lines),
            "space" => Ok(TransformFormat::Space),
            "comma" => Ok(TransformFormat::Comma),
            "json" => Ok(TransformFormat::Json),
            "yaml" => Ok(TransformFormat::Yaml),
            _ => Err(format!("Unknown transform format: {}", s)),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "yaml" | "yml" => Ok(OutputFormat::Yaml),
            "json" => Ok(OutputFormat::Json),
            "json-pretty" => Ok(OutputFormat::JsonPretty),
            _ => Err(format!("Unknown format: {}", s)),
        }
    }
}

/// Initialize a new vm.yaml configuration file
pub fn init_config_file(
    file_path: Option<PathBuf>,
    services: Option<String>,
    ports: Option<u16>,
) -> Result<()> {
    commands::init::execute(file_path, services, ports)
}

pub fn execute(args: Args) -> Result<()> {
    match args.command {
        Command::Merge { base, overlay, format } => {
            commands::merge::execute_merge(base, overlay, format)
        }
        Command::Validate { file, no_preset, verbose } => {
            commands::validation::execute_validate(file, no_preset, verbose)
        }
        Command::Preset { dir, presets_dir, detect_only, list } => {
            commands::preset::execute(dir, presets_dir, detect_only, list)
        }
        Command::Convert { input, format } => {
            commands::conversion::execute(input, format)
        }
        Command::Process { defaults, config, project_dir, presets_dir, format } => {
            commands::process::execute(defaults, config, project_dir, presets_dir, format)
        }
        Command::Query { config, field, raw, default } => {
            commands::query::execute_query(config, field, raw, default)
        }
        Command::ArrayAdd { file, path, item } => {
            commands::file_ops::execute_array_add(file, path, item)
        }
        Command::ArrayRemove { file, path, filter } => {
            commands::file_ops::execute_array_remove(file, path, filter)
        }
        Command::Filter { file, expression, output_format } => {
            commands::query::execute_filter(file, expression, output_format)
        }
        Command::CheckFile { file } => {
            commands::validation::execute_check_file(file)
        }
        Command::MergeEvalAll { files, format } => {
            commands::merge::execute_merge_eval_all(files, format)
        }
        Command::Modify { file, field, value, stdout } => {
            commands::file_ops::execute_modify(file, field, value, stdout)
        }
        Command::ArrayLength { file, path } => {
            commands::query::execute_array_length(file, path)
        }
        Command::Transform { file, expression, format } => {
            commands::transformation::execute(file, expression, format)
        }
        Command::HasField { file, field, subfield } => {
            commands::query::execute_has_field(file, field, subfield)
        }
        Command::AddToArray { file, path, object, stdout } => {
            commands::file_ops::execute_add_to_array(file, path, object, stdout)
        }
        Command::SelectWhere { file, path, field, value, format } => {
            commands::query::execute_select_where(file, path, field, value, format)
        }
        Command::Count { file, path } => {
            commands::query::execute_count(file, path)
        }
        Command::Delete { file, path, field, value, format } => {
            commands::file_ops::execute_delete(file, path, field, value, format)
        }
        Command::Dump { file, no_preset } => {
            commands::dump::execute_dump(file, no_preset)
        }
        Command::Export { file, no_preset } => {
            commands::dump::execute_export(file, no_preset)
        }
        Command::Set { field, value, global } => {
            commands::config_ops::execute_set(field, value, global)
        }
        Command::Get { field, global } => {
            commands::config_ops::execute_get(field, global)
        }
        Command::Unset { field, global } => {
            commands::config_ops::execute_unset(field, global)
        }
        Command::Clear { global } => {
            commands::config_ops::execute_clear(global)
        }
        Command::Init { file, services, ports } => {
            commands::init::execute(file, services, ports)
        }
    }
}