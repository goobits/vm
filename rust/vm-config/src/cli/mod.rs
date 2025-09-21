//! CLI interface for VM configuration management.
//!
//! This module implements a command-line interface for VM configuration operations
//! using a command group pattern for better organization and maintainability.
//!
//! ## Architecture
//!
//! The CLI is organized into command groups for clear separation of concerns:
//! - **FileOpsGroup**: File manipulation commands (merge, convert, modify)
//! - **QueryOpsGroup**: Data querying commands (query, filter, count)
//! - **ConfigOpsGroup**: Configuration management (get, set, validate)
//! - **ProjectOpsGroup**: Project-level operations (init, preset, process)
//!
//! ## Command Flow
//!
//! 1. Command definitions are in the `Command` enum (data structures only)
//! 2. The `execute()` function dispatches to appropriate command groups
//! 3. Command groups delegate to individual command handlers in `commands/`
//! 4. Each handler performs its specific operation and returns results
//!
//! This pattern keeps the main CLI file focused on structure while delegating
//! implementation details to specialized modules.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod command_groups;
mod commands;
mod formatting;

pub use commands::validation::load_and_merge_config;
pub use formatting::*;

// Import command groups for organized dispatch
use command_groups::{ConfigOpsGroup, FileOpsGroup, ProjectOpsGroup, QueryOpsGroup};

/// Command-line arguments for the VM configuration tool.
///
/// This structure defines the top-level CLI interface for vm-config,
/// which provides utilities for processing, validating, and manipulating
/// VM configuration files.
#[derive(Parser)]
#[command(name = "vm-config")]
#[command(about = "Configuration processor for VM Tool")]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

/// Available CLI commands for VM configuration operations.
///
/// This enum defines all supported operations for working with VM configurations,
/// including file manipulation, validation, merging, and querying capabilities.
/// Each variant contains the specific arguments needed for that operation.
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
    },

    /// Load, merge, and validate configuration, outputting as shell export commands.
    Export {
        /// Config file path (optional, searches for vm.yaml if not provided)
        #[arg(short, long)]
        file: Option<PathBuf>,
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

    /// Port range management commands
    #[command(subcommand)]
    Ports(PortsCommand),
}

/// Port management subcommands
#[derive(Subcommand)]
pub enum PortsCommand {
    /// Check for port range conflicts
    Check {
        /// Port range (e.g., "3000-3009")
        range: String,
        /// Optional project name to exclude from conflict checking
        project_name: Option<String>,
    },
    /// Register a port range for a project
    Register {
        /// Port range (e.g., "3000-3009")
        range: String,
        /// Project name
        project: String,
        /// Project path
        path: String,
    },
    /// Suggest next available port range
    Suggest {
        /// Range size (default: 10)
        size: Option<u16>,
    },
    /// List all registered port ranges
    List,
    /// Unregister a project's port range
    Unregister {
        /// Project name
        project: String,
    },
}

/// Output format options for configuration data.
///
/// Determines how configuration data should be formatted when output to stdout.
/// Different commands may support different subsets of these formats.
///
/// # Formats
/// - `Yaml` - Human-readable YAML format (default for most operations)
/// - `Json` - Compact JSON format
/// - `JsonPretty` - Pretty-printed JSON with indentation
#[derive(Clone, Debug)]
pub enum OutputFormat {
    Yaml,
    Json,
    JsonPretty,
}

/// Output format options for data transformation operations.
///
/// Specialized format options for the transform command, which can output
/// data in various formats suitable for shell scripting and data processing.
///
/// # Formats
/// - `Lines` - One item per line (default)
/// - `Space` - Space-separated values
/// - `Comma` - Comma-separated values
/// - `Json` - JSON array format
/// - `Yaml` - YAML array format
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

/// Initialize a new vm.yaml configuration file.
///
/// Creates a new VM configuration file with sensible defaults and optional
/// service configurations. This is typically used to bootstrap new projects
/// with VM tool support.
///
/// ## Generated Configuration
/// - Basic project structure with detected or default settings
/// - Optional service configurations (databases, caches, etc.)
/// - Sequential port allocation for services
/// - Provider-appropriate defaults
///
/// # Arguments
/// * `file_path` - Target file or directory (defaults to current directory/vm.yaml)
/// * `services` - Comma-separated list of services to enable (e.g., "postgresql,redis")
/// * `ports` - Starting port number for service allocation
///
/// # Returns
/// `Ok(())` if the configuration file was created successfully
///
/// # Errors
/// Returns an error if:
/// - File cannot be written
/// - Directory does not exist
/// - Invalid service names provided
///
/// # Examples
/// ```rust,no_run,no_run
/// use vm_config::init_config_file;
/// use std::path::PathBuf;
///
/// // Create basic configuration
/// init_config_file(Some(PathBuf::from("test-vm.yaml")), None, None)?;
///
/// // Create with services
/// init_config_file(
///     Some(PathBuf::from("my-project/vm.yaml")),
///     Some("postgresql,redis".to_string()),
///     Some(5432)
/// )?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn init_config_file(
    file_path: Option<PathBuf>,
    services: Option<String>,
    ports: Option<u16>,
) -> Result<()> {
    commands::init::execute(file_path, services, ports)
}

/// Execute a ports subcommand
fn execute_ports_command(cmd: PortsCommand) -> Result<()> {
    use crate::ports::{PortRange, PortRegistry};
    use vm_common::{vm_error, vm_success, vm_warning};

    match cmd {
        PortsCommand::Check {
            range,
            project_name,
        } => {
            let port_range = PortRange::parse(&range)?;
            let registry = PortRegistry::load()?;

            if let Some(conflicts) = registry.check_conflicts(&port_range, project_name.as_deref())
            {
                println!("{}", conflicts);
                std::process::exit(1);
            } else {
                std::process::exit(0);
            }
        }
        PortsCommand::Register {
            range,
            project,
            path,
        } => {
            let port_range = PortRange::parse(&range)?;
            let mut registry = PortRegistry::load()?;

            if let Some(conflicts) = registry.check_conflicts(&port_range, Some(&project)) {
                vm_warning!("Port range {} conflicts with: {}", range, conflicts);
                std::process::exit(1);
            } else {
                registry.register(&project, &port_range, &path)?;
                vm_success!("Registered port range {} for project '{}'", range, project);
            }
        }
        PortsCommand::Suggest { size } => {
            let registry = PortRegistry::load()?;
            let size = size.unwrap_or(10);

            if let Some(range) = registry.suggest_next_range(size, 3000) {
                println!("{}", range);
            } else {
                vm_error!("No available port range of size {} found", size);
                std::process::exit(1);
            }
        }
        PortsCommand::List => {
            let registry = PortRegistry::load()?;
            registry.list();
        }
        PortsCommand::Unregister { project } => {
            let mut registry = PortRegistry::load()?;
            registry.unregister(&project)?;
            vm_success!("Unregistered port range for project '{}'", project);
        }
    }

    Ok(())
}

/// Execute a CLI command with the provided arguments.
///
/// This is the main command dispatcher that routes CLI arguments to their
/// corresponding implementation functions. It handles all supported VM
/// configuration operations including merging, validation, querying, and
/// file manipulation.
///
/// # Arguments
/// * `args` - Parsed command-line arguments containing the command and its parameters
///
/// # Returns
/// `Ok(())` if the command executed successfully
///
/// # Errors
/// Returns an error if:
/// - Command execution fails
/// - Invalid arguments provided
/// - File operations fail
/// - Configuration parsing errors
///
/// # Examples
/// ```rust,no_run
/// use vm_config::cli::{Args, Command, OutputFormat};
/// use std::path::PathBuf;
///
/// let args = Args {
///     command: Command::Validate {
///         file: Some(PathBuf::from("vm.yaml")),
///         verbose: true,
///     }
/// };
///
/// vm_config::cli::execute(args)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use = "command execution results should be handled"]
pub fn execute(args: Args) -> Result<()> {
    use Command::*;

    match args.command {
        // File Operations Group
        Merge {
            base,
            overlay,
            format,
        } => FileOpsGroup::execute_merge(base, overlay, format),
        Convert { input, format } => FileOpsGroup::execute_convert(input, format),
        ArrayAdd { file, path, item } => FileOpsGroup::execute_array_add(file, path, item),
        ArrayRemove { file, path, filter } => {
            FileOpsGroup::execute_array_remove(file, path, filter)
        }
        Modify {
            file,
            field,
            value,
            stdout,
        } => FileOpsGroup::execute_modify(file, field, value, stdout),
        AddToArray {
            file,
            path,
            object,
            stdout,
        } => FileOpsGroup::execute_add_to_array(file, path, object, stdout),
        Delete {
            file,
            path,
            field,
            value,
            format,
        } => FileOpsGroup::execute_delete(file, path, field, value, format),
        CheckFile { file } => FileOpsGroup::execute_check_file(file),
        MergeEvalAll { files, format } => FileOpsGroup::execute_merge_eval_all(files, format),
        Transform {
            file,
            expression,
            format,
        } => FileOpsGroup::execute_transform(file, expression, format),

        // Query Operations Group
        Query {
            config,
            field,
            raw,
            default,
        } => QueryOpsGroup::execute_query(config, field, raw, default),
        Filter {
            file,
            expression,
            output_format,
        } => QueryOpsGroup::execute_filter(file, expression, output_format),
        ArrayLength { file, path } => QueryOpsGroup::execute_array_length(file, path),
        HasField {
            file,
            field,
            subfield,
        } => QueryOpsGroup::execute_has_field(file, field, subfield),
        SelectWhere {
            file,
            path,
            field,
            value,
            format,
        } => QueryOpsGroup::execute_select_where(file, path, field, value, format),
        Count { file, path } => QueryOpsGroup::execute_count(file, path),

        // Configuration Operations Group
        Set {
            field,
            value,
            global,
        } => ConfigOpsGroup::execute_set(field, value, global),
        Get { field, global } => ConfigOpsGroup::execute_get(field, global),
        Unset { field, global } => ConfigOpsGroup::execute_unset(field, global),
        Validate { file, verbose } => {
            ConfigOpsGroup::execute_validate(file, verbose);
            Ok(())
        }
        Dump { file } => ConfigOpsGroup::execute_dump(file),
        Export { file } => ConfigOpsGroup::execute_export(file),

        // Project Operations Group
        Preset {
            dir,
            presets_dir,
            detect_only,
            list,
        } => ProjectOpsGroup::execute_preset(dir, presets_dir, detect_only, list),
        Process {
            defaults,
            config,
            project_dir,
            presets_dir,
            format,
        } => ProjectOpsGroup::execute_process(defaults, config, project_dir, presets_dir, format),
        Init {
            file,
            services,
            ports,
        } => ProjectOpsGroup::execute_init(file, services, ports),
        Ports(ports_cmd) => execute_ports_command(ports_cmd),
    }
}
