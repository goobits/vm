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

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vm_core::error::Result;

pub mod array_cmd;
mod command_groups;
mod commands;
pub mod config_cmd;
pub mod file_cmd;
mod formatting;
pub mod ports_cmd;
pub mod project_cmd;
pub mod query_cmd;

pub use array_cmd::ArrayCmd;
pub use config_cmd::ConfigCmd;
pub use file_cmd::FileCmd;
pub use formatting::*;
pub use ports_cmd::PortsCmd;
pub use project_cmd::ProjectCmd;
pub use query_cmd::QueryCmd;

pub use commands::validation::load_and_merge_config;

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
    #[command(flatten)]
    Config(ConfigCmd),

    #[command(flatten)]
    Project(ProjectCmd),

    #[command(flatten)]
    File(FileCmd),

    #[command(flatten)]
    Query(QueryCmd),

    #[command(flatten)]
    Array(ArrayCmd),

    /// Port range management commands
    #[command(subcommand)]
    Ports(PortsCmd),
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

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "lines" => Ok(TransformFormat::Lines),
            "space" => Ok(TransformFormat::Space),
            "comma" => Ok(TransformFormat::Comma),
            "json" => Ok(TransformFormat::Json),
            "yaml" => Ok(TransformFormat::Yaml),
            _ => Err(format!("Unknown transform format: {s}")),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "yaml" | "yml" => Ok(OutputFormat::Yaml),
            "json" => Ok(OutputFormat::Json),
            "json-pretty" => Ok(OutputFormat::JsonPretty),
            _ => Err(format!("Unknown format: {s}")),
        }
    }
}

pub fn init_config_file(
    file_path: Option<PathBuf>,
    services: Option<String>,
    ports: Option<u16>,
) -> Result<()> {
    commands::init::execute(file_path, services, ports)
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
/// use vm_config::cli::{Args, Command, ConfigCmd, OutputFormat};
/// use std::path::PathBuf;
///
/// let args = Args {
///     command: Command::Config(ConfigCmd::Validate {
///         file: Some(PathBuf::from("vm.yaml")),
///         verbose: true,
///     }),
/// };
///
/// vm_config::cli::execute(args)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use = "command execution results should be handled"]
pub fn execute(args: Args) -> Result<()> {
    use Command::*;

    match args.command {
        Config(cmd) => execute_config_command(cmd),
        Project(cmd) => execute_project_command(cmd),
        File(cmd) => execute_file_command(cmd),
        Query(cmd) => execute_query_command(cmd),
        Array(cmd) => execute_array_command(cmd),
        Ports(cmd) => execute_ports_command(cmd),
    }
}

fn execute_config_command(cmd: ConfigCmd) -> Result<()> {
    match cmd {
        ConfigCmd::Set {
            field,
            value,
            global,
        } => ConfigOpsGroup::execute_set(field, value, global),
        ConfigCmd::Get { field, global } => ConfigOpsGroup::execute_get(field, global),
        ConfigCmd::Unset { field, global } => ConfigOpsGroup::execute_unset(field, global),
        ConfigCmd::Validate { file, verbose } => {
            ConfigOpsGroup::execute_validate(file, verbose);
            Ok(())
        }
        ConfigCmd::Dump { file } => ConfigOpsGroup::execute_dump(file),
        ConfigCmd::Export { file } => ConfigOpsGroup::execute_export(file),
        ConfigCmd::Migrate => ConfigOpsGroup::execute_migrate(),
    }
}

fn execute_project_command(cmd: ProjectCmd) -> Result<()> {
    match cmd {
        ProjectCmd::Init {
            file,
            services,
            ports,
        } => ProjectOpsGroup::execute_init(file, services, ports),
        ProjectCmd::Preset {
            dir,
            presets_dir,
            detect_only,
            list,
        } => ProjectOpsGroup::execute_preset(dir, presets_dir, detect_only, list),
        ProjectCmd::Process {
            defaults,
            config,
            project_dir,
            presets_dir,
            format,
        } => ProjectOpsGroup::execute_process(defaults, config, project_dir, presets_dir, format),
    }
}

fn execute_file_command(cmd: FileCmd) -> Result<()> {
    match cmd {
        FileCmd::Merge {
            base,
            overlay,
            format,
        } => FileOpsGroup::execute_merge(base, overlay, format),
        FileCmd::Convert { input, format } => FileOpsGroup::execute_convert(input, format),
        FileCmd::Modify {
            file,
            field,
            value,
            stdout,
        } => FileOpsGroup::execute_modify(file, field, value, stdout),
        FileCmd::CheckFile { file } => FileOpsGroup::execute_check_file(file),
        FileCmd::MergeEvalAll { files, format } => {
            FileOpsGroup::execute_merge_eval_all(files, format)
        }
    }
}

fn execute_query_command(cmd: QueryCmd) -> Result<()> {
    match cmd {
        QueryCmd::Query {
            config,
            field,
            raw,
            default,
        } => QueryOpsGroup::execute_query(config, field, raw, default),
        QueryCmd::Filter {
            file,
            expression,
            output_format,
        } => QueryOpsGroup::execute_filter(file, expression, output_format),
        QueryCmd::Count { file, path } => QueryOpsGroup::execute_count(file, path),
        QueryCmd::SelectWhere {
            file,
            path,
            field,
            value,
            format,
        } => QueryOpsGroup::execute_select_where(file, path, field, value, format),
        QueryCmd::HasField {
            file,
            field,
            subfield,
        } => QueryOpsGroup::execute_has_field(file, field, subfield),
        QueryCmd::Transform {
            file,
            expression,
            format,
        } => FileOpsGroup::execute_transform(file, expression, format),
    }
}

fn execute_array_command(cmd: ArrayCmd) -> Result<()> {
    match cmd {
        ArrayCmd::ArrayAdd { file, path, item } => {
            FileOpsGroup::execute_array_add(file, path, item)
        }
        ArrayCmd::ArrayRemove { file, path, filter } => {
            FileOpsGroup::execute_array_remove(file, path, filter)
        }
        ArrayCmd::ArrayLength { file, path } => QueryOpsGroup::execute_array_length(file, path),
        ArrayCmd::AddToArray {
            file,
            path,
            object,
            stdout,
        } => FileOpsGroup::execute_add_to_array(file, path, object, stdout),
        ArrayCmd::Delete {
            file,
            path,
            field,
            value,
            format,
        } => FileOpsGroup::execute_delete(file, path, field, value, format),
    }
}

fn execute_ports_command(cmd: PortsCmd) -> Result<()> {
    use crate::ports::{PortRange, PortRegistry};
    use vm_core::{vm_error, vm_success, vm_warning};

    match cmd {
        PortsCmd::Check {
            range,
            project_name,
        } => {
            let port_range = PortRange::parse(&range)?;
            let registry = PortRegistry::load()?;

            if let Some(conflicts) = registry.check_conflicts(&port_range, project_name.as_deref())
            {
                println!("{conflicts}");
                std::process::exit(1);
            } else {
                std::process::exit(0);
            }
        }
        PortsCmd::Register {
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
        PortsCmd::Suggest { size } => {
            let registry = PortRegistry::load()?;
            let size = size.unwrap_or(10);

            if let Some(range) = registry.suggest_next_range(size, 3000) {
                println!("{range}");
            } else {
                vm_error!("No available port range of size {} found", size);
                std::process::exit(1);
            }
        }
        PortsCmd::List => {
            let registry = PortRegistry::load()?;
            registry.list();
        }
        PortsCmd::Unregister { project } => {
            let mut registry = PortRegistry::load()?;
            registry.unregister(&project)?;
            vm_success!("Unregistered port range for project '{}'", project);
        }
    }
    Ok(())
}
