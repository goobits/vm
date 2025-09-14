use crate::{config::VmConfig, merge, preset::PresetDetector, validate::ConfigValidator, paths};
use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
        /// Config file to validate
        config: PathBuf,

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

pub fn execute(args: Args) -> Result<()> {
    match args.command {
        Command::Merge { base, overlay, format } => {
            let base_config = VmConfig::from_file(&base)
                .with_context(|| format!("Failed to load base config: {:?}", base))?;

            let mut overlays = Vec::new();
            for path in overlay {
                let config = VmConfig::from_file(&path)
                    .with_context(|| format!("Failed to load overlay: {:?}", path))?;
                overlays.push(config);
            }

            let merged = merge::ConfigMerger::new(base_config).merge_all(overlays)?;
            output_config(&merged, &format)?;
        }

        Command::Validate { config, verbose } => {
            let config = VmConfig::from_file(&config)
                .with_context(|| format!("Failed to load config: {:?}", config))?;

            let validator = ConfigValidator::new(config);
            match validator.validate() {
                Ok(_) => {
                    println!("✅ Configuration is valid");
                    if verbose {
                        println!("All validation checks passed");
                    }
                }
                Err(e) => {
                    eprintln!("❌ Configuration validation failed: {}", e);
                    if verbose {
                        eprintln!("\nDetails: {:?}", e);
                    }
                    std::process::exit(1);
                }
            }
        }

        Command::Preset { dir, presets_dir, detect_only, list } => {
            let presets_dir = presets_dir.unwrap_or_else(paths::get_presets_dir);
            let detector = PresetDetector::new(dir.clone(), presets_dir);

            if list {
                let presets = detector.list_presets()?;
                println!("Available presets:");
                for preset in presets {
                    println!("  - {}", preset);
                }
            } else if detect_only {
                match detector.detect()? {
                    Some(preset) => println!("{}", preset),
                    None => println!("base"),
                }
            } else {
                match detector.detect()? {
                    Some(preset_name) => {
                        let preset = detector.load_preset(&preset_name)?;
                        output_config(&preset, &OutputFormat::Yaml)?;
                    }
                    None => {
                        eprintln!("No preset detected for project");
                        std::process::exit(1);
                    }
                }
            }
        }

        Command::Convert { input, format } => {
            let config = VmConfig::from_file(&input)
                .with_context(|| format!("Failed to load config: {:?}", input))?;
            output_config(&config, &format)?;
        }

        Command::Process { defaults, config, project_dir, presets_dir, format } => {
            // Use default paths if not specified
            let defaults = defaults.unwrap_or_else(|| paths::resolve_tool_path("vm.yaml"));
            let presets_dir = presets_dir.unwrap_or_else(paths::get_presets_dir);

            // Load default config
            let default_config = VmConfig::from_file(&defaults)
                .with_context(|| format!("Failed to load defaults: {:?}", defaults))?;

            // Load user config if provided
            let user_config = if let Some(path) = config {
                Some(VmConfig::from_file(&path)
                    .with_context(|| format!("Failed to load user config: {:?}", path))?)
            } else {
                None
            };

            // Detect and load preset if user config is partial
            let preset_config = if user_config.as_ref().map_or(true, |c| c.is_partial()) {
                let detector = PresetDetector::new(project_dir, presets_dir);
                if let Some(preset_name) = detector.detect()? {
                    Some(detector.load_preset(&preset_name)?)
                } else {
                    None
                }
            } else {
                None
            };

            // Merge in order: defaults -> preset -> user
            let merged = merge::merge_configs(Some(default_config), preset_config, user_config)?;
            output_config(&merged, &format)?;
        }

        Command::Query { config, field, raw, default } => {
            let config = VmConfig::from_file(&config)
                .with_context(|| format!("Failed to load config: {:?}", config))?;

            let json_value = serde_json::to_value(&config)?;
            let value = match query_field(&json_value, &field) {
                Ok(val) => {
                    if val.is_null() && default.is_some() {
                        serde_json::Value::String(default.expect("Default value should be available when checked"))
                    } else {
                        val
                    }
                },
                Err(_) => {
                    if let Some(default_val) = default {
                        serde_json::Value::String(default_val)
                    } else {
                        return Err(anyhow::anyhow!("Field not found: {}", field));
                    }
                }
            };

            if raw && value.is_string() {
                println!("{}", value.as_str().expect("String value should have string content"));
            } else {
                println!("{}", serde_json::to_string(&value)?);
            }
        }

        Command::ArrayAdd { file, path, item } => {
            use crate::yaml_ops::YamlOperations;
            YamlOperations::array_add(&file, &path, &item)?;
        }

        Command::ArrayRemove { file, path, filter } => {
            use crate::yaml_ops::YamlOperations;
            YamlOperations::array_remove(&file, &path, &filter)?;
        }

        Command::Filter { file, expression, output_format } => {
            use crate::yaml_ops::YamlOperations;
            YamlOperations::filter(&file, &expression, &output_format)?;
        }

        Command::CheckFile { file } => {
            use crate::yaml_ops::YamlOperations;
            match YamlOperations::validate_file(&file) {
                Ok(_) => {
                    println!("✅ File is valid YAML");
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("❌ File validation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Command::MergeEvalAll { files, format } => {
            use crate::yaml_ops::YamlOperations;
            YamlOperations::merge_eval_all(&files, &format)?;
        }


        Command::Modify { file, field, value, stdout } => {
            use crate::yaml_ops::YamlOperations;
            YamlOperations::modify_file(&file, &field, &value, stdout)?;
        }

        Command::ArrayLength { file, path } => {
            use crate::yaml_ops::YamlOperations;
            let length = YamlOperations::array_length(&file, &path)?;
            println!("{}", length);
        }

        Command::Transform { file, expression, format } => {
            use crate::yaml_ops::YamlOperations;
            YamlOperations::transform(&file, &expression, &format)?;
        }

        Command::HasField { file, field, subfield } => {
            use crate::yaml_ops::YamlOperations;
            match YamlOperations::has_field(&file, &field, &subfield) {
                Ok(true) => {
                    println!("true");
                    std::process::exit(0);
                }
                Ok(false) => {
                    println!("false");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("❌ Error checking field: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Command::AddToArray { file, path, object, stdout } => {
            use crate::yaml_ops::YamlOperations;
            YamlOperations::add_to_array_path(&file, &path, &object, stdout)?;
        }

        Command::SelectWhere { file, path, field, value, format } => {
            use crate::yaml_ops::YamlOperations;
            YamlOperations::select_where(&file, &path, &field, &value, &format)?;
        }

        Command::Count { file, path } => {
            use crate::yaml_ops::YamlOperations;
            let count = YamlOperations::count_items(&file, &path)?;
            println!("{}", count);
        }

        Command::Delete { file, path, field, value, format } => {
            use crate::yaml_ops::YamlOperations;
            YamlOperations::delete_from_array(&file, &path, &field, &value, &format)?;
        }

        Command::Dump { file } => {
            // Configuration discovery and loading
            let config_path = if let Some(path) = file {
                path
            } else {
                // Search for vm.yaml in current directory and upwards
                find_vm_config_file()?
            };

            // Use the default vm.yaml path for defaults
            let defaults_path = crate::paths::resolve_tool_path("vm.yaml");
            let presets_dir = crate::paths::get_presets_dir();

            // Determine project directory (parent of found config file)
            let project_dir = config_path.parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf();

            // Load default config
            let default_config = VmConfig::from_file(&defaults_path)
                .with_context(|| format!("Failed to load defaults from: {:?}", defaults_path))?;

            // Load user config
            let user_config = VmConfig::from_file(&config_path)
                .with_context(|| format!("Failed to load config from: {:?}", config_path))?;

            // Detect and load preset if user config is partial
            let preset_config = if user_config.is_partial() {
                let detector = crate::preset::PresetDetector::new(project_dir, presets_dir);
                if let Some(preset_name) = detector.detect()? {
                    Some(detector.load_preset(&preset_name)?)
                } else {
                    None
                }
            } else {
                None
            };

            // Merge in order: defaults -> preset -> user
            let merged = crate::merge::merge_configs(Some(default_config), preset_config, Some(user_config))?;

            // Validate the merged configuration
            let validator = crate::validate::ConfigValidator::new(merged.clone());
            validator.validate().with_context(|| "Configuration validation failed")?;

            // Output as YAML
            let yaml = serde_yaml::to_string(&merged)?;
            print!("{}", yaml);
        }
    }

    Ok(())
}

fn output_config(config: &VmConfig, format: &OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Yaml => {
            let yaml = serde_yaml::to_string(config)?;
            print!("{}", yaml);
        }
        OutputFormat::Json => {
            let json = serde_json::to_string(config)?;
            println!("{}", json);
        }
        OutputFormat::JsonPretty => {
            let json = config.to_json()?;
            println!("{}", json);
        }
    }
    Ok(())
}

fn query_field(value: &serde_json::Value, field: &str) -> Result<serde_json::Value> {
    let parts: Vec<&str> = field.split('.').collect();
    let mut current = value;

    for part in parts {
        match current {
            serde_json::Value::Object(map) => {
                current = map.get(part)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", part))?;
            }
            _ => anyhow::bail!("Cannot access field '{}' on non-object", part),
        }
    }

    Ok(current.clone())
}

/// Find vm.yaml by searching current directory and upwards
fn find_vm_config_file() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    let mut dir = current_dir.as_path();

    loop {
        let config_path = dir.join("vm.yaml");
        if config_path.exists() {
            return Ok(config_path);
        }

        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    anyhow::bail!("Could not find vm.yaml file in current directory or any parent directory");
}