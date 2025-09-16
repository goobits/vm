use crate::config::VmConfig;
use anyhow::{Context, Result};
use serde_yaml::Value;
use std::path::{Path, PathBuf};

use super::{OutputFormat, TransformFormat};

pub fn output_config(config: &VmConfig, format: &OutputFormat) -> Result<()> {
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

pub fn query_field(value: &serde_json::Value, field: &str) -> Result<serde_json::Value> {
    let parts: Vec<&str> = field.split('.').collect();
    let mut current = value;

    for part in parts {
        match current {
            serde_json::Value::Object(map) => {
                current = map
                    .get(part)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", part))?;
            }
            _ => anyhow::bail!("Cannot access field '{}' on non-object", part),
        }
    }

    Ok(current.clone())
}

/// Find vm.yaml by searching current directory and upwards
pub fn find_vm_config_file() -> Result<PathBuf> {
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

pub fn output_shell_exports(value: &Value) -> Result<()> {
    let mut exports = Vec::new();
    flatten_yaml_to_shell("", value, &mut exports);
    for export in exports {
        println!("{}", export);
    }
    Ok(())
}

fn flatten_yaml_to_shell(prefix: &str, value: &Value, exports: &mut Vec<String>) {
    match value {
        Value::Mapping(map) => {
            for (key, val) in map {
                if let Value::String(key_str) = key {
                    // Sanitize key for shell variable names (replace hyphens with underscores)
                    let sanitized_key = key_str.replace('-', "_");
                    let new_prefix = if prefix.is_empty() {
                        sanitized_key
                    } else {
                        format!("{}_{}", prefix, sanitized_key)
                    };
                    flatten_yaml_to_shell(&new_prefix, val, exports);
                }
            }
        }
        Value::String(s) => {
            // Properly escape shell metacharacters for safe export
            let escaped = s
                .replace('\\', "\\\\") // Escape backslash first
                .replace('"', "\\\"") // Escape double quotes
                .replace('$', "\\$") // Escape dollar signs (prevents variable expansion)
                .replace('`', "\\`"); // Escape backticks (prevents command substitution)
            exports.push(format!("export {}=\"{}\"", prefix, escaped));
        }
        Value::Bool(b) => {
            exports.push(format!("export {}={}", prefix, b));
        }
        Value::Number(n) => {
            exports.push(format!("export {}={}", prefix, n));
        }
        // Sequences (arrays) and nulls are ignored for shell export
        _ => {}
    }
}