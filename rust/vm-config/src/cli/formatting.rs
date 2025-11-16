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

/// Escape shell special characters in a value
fn escape_shell_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}

/// Build a full shell variable name from prefix and key
fn build_shell_var_name(prefix: &str, key: &str) -> String {
    let sanitized_key = key.replace('-', "_");
    if prefix.is_empty() {
        sanitized_key
    } else {
        format!("{prefix}_{sanitized_key}")
    }
}

/// Add a string export to the exports list
fn add_string_export(exports: &mut Vec<String>, prefix: &str, key: &str, value: &str) {
    let full_key = build_shell_var_name(prefix, key);
    let escaped = escape_shell_value(value);
    exports.push(format!("export {full_key}=\"{escaped}\""));
}

/// Add a numeric export to the exports list
fn add_numeric_export(exports: &mut Vec<String>, prefix: &str, key: &str, value: &str) {
    let full_key = build_shell_var_name(prefix, key);
    exports.push(format!("export {full_key}={value}"));
}

/// Add a boolean export to the exports list
fn add_boolean_export(exports: &mut Vec<String>, prefix: &str, key: &str, value: bool) {
    add_numeric_export(exports, prefix, key, &value.to_string());
}

/// Export optional string fields from VmConfig
fn export_optional_strings(exports: &mut Vec<String>, prefix: &str, config: &VmConfig) {
    if let Some(ref schema) = config.schema {
        add_string_export(exports, prefix, "schema", schema);
    }
    if let Some(ref version) = config.version {
        add_string_export(exports, prefix, "version", version);
    }
    if let Some(ref provider) = config.provider {
        add_string_export(exports, prefix, "provider", provider);
    }
    if let Some(ref os) = config.os {
        add_string_export(exports, prefix, "os", os);
    }
}

/// Export package arrays from VmConfig
fn export_package_arrays(exports: &mut Vec<String>, prefix: &str, config: &VmConfig) {
    for (i, package) in config.apt_packages.iter().enumerate() {
        add_string_export(exports, prefix, &format!("apt_packages_{i}"), package);
    }
    for (i, package) in config.npm_packages.iter().enumerate() {
        add_string_export(exports, prefix, &format!("npm_packages_{i}"), package);
    }
    for (i, package) in config.pip_packages.iter().enumerate() {
        add_string_export(exports, prefix, &format!("pip_packages_{i}"), package);
    }
    for (i, package) in config.cargo_packages.iter().enumerate() {
        add_string_export(exports, prefix, &format!("cargo_packages_{i}"), package);
    }
}

/// Flatten VmConfig directly to shell exports without serialization overhead
fn flatten_config_to_shell(prefix: &str, config: &VmConfig, exports: &mut Vec<String>) {
    // Export optional string fields
    export_optional_strings(exports, prefix, config);

    // Handle port range
    if let Some(range) = &config.ports.range {
        if range.len() == 2 {
            add_numeric_export(exports, prefix, "port_range_start", &range[0].to_string());
            add_numeric_export(exports, prefix, "port_range_end", &range[1].to_string());
        }
    }

    // Export service ports
    for (service_name, service_config) in &config.services {
        if let Some(port) = service_config.port {
            let port_key = format!("service_port_{}", service_name.replace('-', "_"));
            add_numeric_export(exports, prefix, &port_key, &port.to_string());
        }
    }

    // Handle aliases and environment maps
    for (alias_name, alias_value) in &config.aliases {
        let alias_key = format!("aliases_{}", alias_name.replace('-', "_"));
        add_string_export(exports, prefix, &alias_key, alias_value);
    }
    for (env_name, env_value) in &config.environment {
        let env_key = format!("environment_{}", env_name.replace('-', "_"));
        add_string_export(exports, prefix, &env_key, env_value);
    }

    // Export package arrays
    export_package_arrays(exports, prefix, config);

    // AI sync - export individual tool flags if configured
    if let Some(ai_sync) = &config
        .host_sync
        .as_ref()
        .and_then(|hs| hs.ai_tools.as_ref())
    {
        add_boolean_export(exports, prefix, "claude_sync", ai_sync.is_claude_enabled());
        add_boolean_export(exports, prefix, "gemini_sync", ai_sync.is_gemini_enabled());
        add_boolean_export(exports, prefix, "codex_sync", ai_sync.is_codex_enabled());
        add_boolean_export(exports, prefix, "cursor_sync", ai_sync.is_cursor_enabled());
        add_boolean_export(exports, prefix, "aider_sync", ai_sync.is_aider_enabled());
    }
}
