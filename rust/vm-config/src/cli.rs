use crate::{config::VmConfig, merge, preset::PresetDetector, validate::ConfigValidator};
use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "vm-config")]
#[command(about = "High-performance YAML configuration processor for VM Tool")]
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

        /// Presets directory
        #[arg(long, default_value = "/workspace/configs/presets")]
        presets_dir: PathBuf,

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
        /// Default config
        #[arg(short, long, default_value = "/workspace/vm.yaml")]
        defaults: PathBuf,

        /// User config (optional)
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Project directory for preset detection
        #[arg(short, long, default_value = ".")]
        project_dir: PathBuf,

        /// Presets directory
        #[arg(long, default_value = "/workspace/configs/presets")]
        presets_dir: PathBuf,

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
    },
}

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Yaml,
    Json,
    JsonPretty,
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

        Command::Query { config, field, raw } => {
            let config = VmConfig::from_file(&config)
                .with_context(|| format!("Failed to load config: {:?}", config))?;

            let json_value = serde_json::to_value(&config)?;
            let value = query_field(&json_value, &field)
                .with_context(|| format!("Field not found: {}", field))?;

            if raw && value.is_string() {
                println!("{}", value.as_str().unwrap());
            } else {
                println!("{}", serde_json::to_string(&value)?);
            }
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
            let json = serde_json::to_string_pretty(config)?;
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