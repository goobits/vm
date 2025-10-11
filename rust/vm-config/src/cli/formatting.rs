use crate::config::VmConfig;
use serde_yaml::Value;
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;
use vm_core::error::{Result, VmError};
use vm_core::vm_error;

use super::OutputFormat;

pub fn output_config(config: &VmConfig, format: &OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Yaml => {
            let yaml = serde_yaml::to_string(config)?;
            print!("{yaml}");
        }
        OutputFormat::Json => {
            let json = serde_json::to_string(config)?;
            println!("{json}");
        }
        OutputFormat::JsonPretty => {
            let json = config.to_json()?;
            println!("{json}");
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
                    .ok_or_else(|| VmError::Config(format!("Field '{part}' not found")))?;
            }
            _ => {
                vm_error!("Cannot access field '{}' on non-object", part);
                return Err(VmError::Config(
                    "Cannot access field on non-object".to_string(),
                ));
            }
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

    // Don't log this as an error - it's a normal situation
    // Just return a clean error that can be handled appropriately
    Err(VmError::Config(
        "No vm.yaml found in current or parent directories".to_string(),
    ))
}

pub fn output_shell_exports(value: &Value) {
    let mut exports = Vec::new();
    flatten_yaml_to_shell("", value, &mut exports);
    for export in exports {
        println!("{export}");
    }
}

/// Optimized shell export output that works directly with VmConfig
/// This avoids the expensive serialization to Value
pub fn output_shell_exports_from_config(config: &VmConfig) {
    let mut exports = Vec::new();
    flatten_config_to_shell("", config, &mut exports);
    for export in exports {
        println!("{export}");
    }
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
                        format!("{prefix}_{sanitized_key}")
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
            exports.push(format!("export {prefix}=\"{escaped}\""));
        }
        Value::Bool(b) => {
            exports.push(format!("export {prefix}={b}"));
        }
        Value::Number(n) => {
            exports.push(format!("export {prefix}={n}"));
        }
        // Sequences (arrays) and nulls are ignored for shell export
        _ => {}
    }
}

/// Flatten VmConfig directly to shell exports without serialization overhead
fn flatten_config_to_shell(prefix: &str, config: &VmConfig, exports: &mut Vec<String>) {
    let add_export = |exports: &mut Vec<String>, key: &str, value: &str| {
        let sanitized_key = key.replace('-', "_");
        let full_key = if prefix.is_empty() {
            sanitized_key
        } else {
            format!("{prefix}_{sanitized_key}")
        };
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`");
        exports.push(format!("export {full_key}=\"{escaped}\""));
    };

    let add_export_num = |exports: &mut Vec<String>, key: &str, value: &str| {
        let sanitized_key = key.replace('-', "_");
        let full_key = if prefix.is_empty() {
            sanitized_key
        } else {
            format!("{prefix}_{sanitized_key}")
        };
        exports.push(format!("export {full_key}={value}"));
    };

    let add_export_bool = |exports: &mut Vec<String>, key: &str, value: bool| {
        add_export_num(exports, key, &value.to_string());
    };

    // Handle all VmConfig fields
    if let Some(ref schema) = config.schema {
        add_export(exports, "schema", schema);
    }
    if let Some(ref version) = config.version {
        add_export(exports, "version", version);
    }
    if let Some(ref provider) = config.provider {
        add_export(exports, "provider", provider);
    }
    if let Some(ref os) = config.os {
        add_export(exports, "os", os);
    }

    // Handle port range
    if let Some(range) = &config.ports.range {
        if range.len() == 2 {
            add_export_num(exports, "port_range_start", &range[0].to_string());
            add_export_num(exports, "port_range_end", &range[1].to_string());
        }
    }

    // Export service ports
    for (service_name, service_config) in &config.services {
        if let Some(port) = service_config.port {
            let port_key = format!("service_port_{}", service_name.replace('-', "_"));
            add_export_num(exports, &port_key, &port.to_string());
        }
    }

    // Handle aliases map
    for (alias_name, alias_value) in &config.aliases {
        let alias_key = format!("aliases_{}", alias_name.replace('-', "_"));
        add_export(exports, &alias_key, alias_value);
    }

    // Handle environment map
    for (env_name, env_value) in &config.environment {
        let env_key = format!("environment_{}", env_name.replace('-', "_"));
        add_export(exports, &env_key, env_value);
    }

    // Handle package arrays
    for (i, package) in config.apt_packages.iter().enumerate() {
        add_export(exports, &format!("apt_packages_{i}"), package);
    }
    for (i, package) in config.npm_packages.iter().enumerate() {
        add_export(exports, &format!("npm_packages_{i}"), package);
    }
    for (i, package) in config.pip_packages.iter().enumerate() {
        add_export(exports, &format!("pip_packages_{i}"), package);
    }
    for (i, package) in config.cargo_packages.iter().enumerate() {
        add_export(exports, &format!("cargo_packages_{i}"), package);
    }

    // Handle boolean flags
    add_export_bool(exports, "claude_sync", config.claude_sync);
    add_export_bool(exports, "gemini_sync", config.gemini_sync);

    // For complex nested structures (project, vm, services, etc.),
    // we'd need to implement similar flattening logic, but the current implementation
    // handles the most common cases. The remaining complex structures can fall back
    // to the Value-based approach if needed.
}
