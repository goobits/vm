//! Schema-aware type detection for configuration values
//!
//! This module provides functionality to determine the expected type of a configuration
//! field based on the YAML schema files. This enables intelligent value parsing in commands
//! like `vm config set`, where array fields can automatically wrap single values in arrays.

use once_cell::sync::Lazy;
use serde_yaml_ng::Value;
use std::collections::HashMap;
use vm_core::error::{Result, VmError};

/// Schema type information for a field
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaType {
    String,
    Integer,
    Boolean,
    Array { item_type: Box<SchemaType> },
    Object,
    Unknown,
}

/// Schema cache for fast lookups
static VM_SCHEMA_CACHE: Lazy<HashMap<String, SchemaType>> = Lazy::new(build_vm_schema_cache);
static GLOBAL_SCHEMA_CACHE: Lazy<HashMap<String, SchemaType>> =
    Lazy::new(build_global_schema_cache);

/// Build the VM schema cache from embedded schema
fn build_vm_schema_cache() -> HashMap<String, SchemaType> {
    let mut cache = HashMap::new();

    // Simple scalar types
    cache.insert("version".to_string(), SchemaType::String);
    cache.insert("os".to_string(), SchemaType::String);
    cache.insert("provider".to_string(), SchemaType::String);

    // Project fields
    cache.insert("project.name".to_string(), SchemaType::String);
    cache.insert("project.hostname".to_string(), SchemaType::String);
    cache.insert("project.workspace_path".to_string(), SchemaType::String);
    cache.insert("project.env_template_path".to_string(), SchemaType::String);
    cache.insert("project.backup_pattern".to_string(), SchemaType::String);

    // VM fields
    cache.insert("vm.box".to_string(), SchemaType::String);
    cache.insert("vm.memory".to_string(), SchemaType::String); // Supports: 1024, "1gb", "50%", "unlimited"
    cache.insert("vm.cpus".to_string(), SchemaType::String); // Supports: 4, "50%", "unlimited"
    cache.insert("vm.user".to_string(), SchemaType::String);
    cache.insert("vm.port_binding".to_string(), SchemaType::String);
    cache.insert("vm.gui".to_string(), SchemaType::Boolean);
    cache.insert("vm.timezone".to_string(), SchemaType::String);
    cache.insert("vm.swap".to_string(), SchemaType::String); // Supports: 1024, "1gb", "50%", "unlimited"
    cache.insert("vm.swappiness".to_string(), SchemaType::Integer);

    // Version fields
    cache.insert("versions.node".to_string(), SchemaType::String);
    cache.insert("versions.nvm".to_string(), SchemaType::String);
    cache.insert("versions.pnpm".to_string(), SchemaType::String);
    cache.insert("versions.python".to_string(), SchemaType::String);

    // Port fields
    cache.insert(
        "ports._range".to_string(),
        SchemaType::Array {
            item_type: Box::new(SchemaType::Integer),
        },
    );

    // Tart fields
    cache.insert("tart.image".to_string(), SchemaType::String);
    cache.insert("tart.guest_os".to_string(), SchemaType::String);
    cache.insert("tart.disk_size".to_string(), SchemaType::String); // Supports: 40, "40gb", "50%"
    cache.insert("tart.rosetta".to_string(), SchemaType::Boolean);
    cache.insert("tart.ssh_user".to_string(), SchemaType::String);
    cache.insert("tart.install_docker".to_string(), SchemaType::Boolean);
    cache.insert("tart.storage_path".to_string(), SchemaType::String);

    // Service fields (common pattern)
    for service in &[
        "postgresql",
        "redis",
        "mongodb",
        "mysql",
        "docker",
        "headless_browser",
        "audio",
        "gpu",
    ] {
        cache.insert(format!("services.{}.enabled", service), SchemaType::Boolean);
        cache.insert(format!("services.{}.port", service), SchemaType::Integer);
    }

    // PostgreSQL/MySQL specific
    cache.insert("services.postgresql.user".to_string(), SchemaType::String);
    cache.insert(
        "services.postgresql.password".to_string(),
        SchemaType::String,
    );
    cache.insert(
        "services.postgresql.database".to_string(),
        SchemaType::String,
    );
    cache.insert("services.mysql.user".to_string(), SchemaType::String);
    cache.insert("services.mysql.password".to_string(), SchemaType::String);
    cache.insert("services.mysql.database".to_string(), SchemaType::String);

    // Docker service specific
    cache.insert("services.docker.buildx".to_string(), SchemaType::Boolean);

    // Headless browser specific
    cache.insert(
        "services.headless_browser.display".to_string(),
        SchemaType::String,
    );
    cache.insert(
        "services.headless_browser.executable_path".to_string(),
        SchemaType::String,
    );

    // Audio service specific
    cache.insert("services.audio.driver".to_string(), SchemaType::String);
    cache.insert(
        "services.audio.share_microphone".to_string(),
        SchemaType::Boolean,
    );

    // GPU service specific
    cache.insert("services.gpu.type".to_string(), SchemaType::String);
    cache.insert("services.gpu.memory_mb".to_string(), SchemaType::Integer);

    // Terminal fields
    cache.insert("terminal.emoji".to_string(), SchemaType::String);
    cache.insert("terminal.username".to_string(), SchemaType::String);
    cache.insert("terminal.theme".to_string(), SchemaType::String);
    cache.insert("terminal.show_git_branch".to_string(), SchemaType::Boolean);
    cache.insert("terminal.show_timestamp".to_string(), SchemaType::Boolean);

    // Package arrays
    cache.insert(
        "apt_packages".to_string(),
        SchemaType::Array {
            item_type: Box::new(SchemaType::String),
        },
    );
    cache.insert(
        "npm_packages".to_string(),
        SchemaType::Array {
            item_type: Box::new(SchemaType::String),
        },
    );
    cache.insert(
        "cargo_packages".to_string(),
        SchemaType::Array {
            item_type: Box::new(SchemaType::String),
        },
    );
    cache.insert(
        "pip_packages".to_string(),
        SchemaType::Array {
            item_type: Box::new(SchemaType::String),
        },
    );

    // Object types (key-value maps)
    cache.insert("aliases".to_string(), SchemaType::Object);
    cache.insert("environment".to_string(), SchemaType::Object);

    // Host synchronization (v2.0)
    cache.insert("host_sync.git_config".to_string(), SchemaType::Boolean);
    cache.insert("host_sync.ssh_agent".to_string(), SchemaType::Boolean);
    cache.insert("host_sync.ssh_config".to_string(), SchemaType::Boolean);
    cache.insert(
        "host_sync.dotfiles".to_string(),
        SchemaType::Array {
            item_type: Box::new(SchemaType::String),
        },
    );
    cache.insert("host_sync.ai_tools".to_string(), SchemaType::Boolean); // Can also be object
    cache.insert(
        "host_sync.package_links.npm".to_string(),
        SchemaType::Boolean,
    );
    cache.insert(
        "host_sync.package_links.pip".to_string(),
        SchemaType::Boolean,
    );
    cache.insert(
        "host_sync.package_links.cargo".to_string(),
        SchemaType::Boolean,
    );
    cache.insert(
        "host_sync.worktrees.enabled".to_string(),
        SchemaType::Boolean,
    );
    cache.insert(
        "host_sync.worktrees.base_path".to_string(),
        SchemaType::String,
    );

    // Networking
    cache.insert(
        "networking.networks".to_string(),
        SchemaType::Array {
            item_type: Box::new(SchemaType::String),
        },
    );

    cache
}

/// Build the global schema cache
fn build_global_schema_cache() -> HashMap<String, SchemaType> {
    let mut cache = HashMap::new();

    // Services
    cache.insert(
        "services.docker_registry.enabled".to_string(),
        SchemaType::Boolean,
    );
    cache.insert(
        "services.docker_registry.port".to_string(),
        SchemaType::Integer,
    );
    cache.insert(
        "services.docker_registry.max_cache_size_gb".to_string(),
        SchemaType::Integer,
    );
    cache.insert(
        "services.docker_registry.max_image_age_days".to_string(),
        SchemaType::Integer,
    );
    cache.insert(
        "services.docker_registry.cleanup_interval_hours".to_string(),
        SchemaType::Integer,
    );
    cache.insert(
        "services.docker_registry.enable_lru_eviction".to_string(),
        SchemaType::Boolean,
    );
    cache.insert(
        "services.docker_registry.enable_auto_restart".to_string(),
        SchemaType::Boolean,
    );
    cache.insert(
        "services.docker_registry.health_check_interval_minutes".to_string(),
        SchemaType::Integer,
    );

    cache.insert(
        "services.auth_proxy.enabled".to_string(),
        SchemaType::Boolean,
    );
    cache.insert("services.auth_proxy.port".to_string(), SchemaType::Integer);
    cache.insert(
        "services.auth_proxy.token_expiry_hours".to_string(),
        SchemaType::Integer,
    );

    cache.insert(
        "services.package_registry.enabled".to_string(),
        SchemaType::Boolean,
    );
    cache.insert(
        "services.package_registry.port".to_string(),
        SchemaType::Integer,
    );
    cache.insert(
        "services.package_registry.max_storage_gb".to_string(),
        SchemaType::Integer,
    );

    // Defaults
    cache.insert("defaults.provider".to_string(), SchemaType::String);
    cache.insert("defaults.memory".to_string(), SchemaType::Integer);
    cache.insert("defaults.cpus".to_string(), SchemaType::Integer);
    cache.insert("defaults.user".to_string(), SchemaType::String);
    cache.insert("defaults.terminal.theme".to_string(), SchemaType::String);
    cache.insert("defaults.terminal.shell".to_string(), SchemaType::String);

    // Features
    cache.insert(
        "features.auto_detect_presets".to_string(),
        SchemaType::Boolean,
    );
    cache.insert(
        "features.auto_port_allocation".to_string(),
        SchemaType::Boolean,
    );
    cache.insert("features.telemetry".to_string(), SchemaType::Boolean);
    cache.insert(
        "features.update_notifications".to_string(),
        SchemaType::Boolean,
    );

    // Worktrees
    cache.insert("worktrees.enabled".to_string(), SchemaType::Boolean);
    cache.insert("worktrees.base_path".to_string(), SchemaType::String);

    cache
}

/// Look up the schema type for a given field path
pub fn lookup_field_type(field: &str, global: bool) -> SchemaType {
    let cache = if global {
        &GLOBAL_SCHEMA_CACHE
    } else {
        &VM_SCHEMA_CACHE
    };

    // Exact match
    if let Some(schema_type) = cache.get(field) {
        return schema_type.clone();
    }

    // Pattern matching for dynamic fields
    // Port assignments: ports.* (except _range)
    if field.starts_with("ports.") && !field.ends_with("._range") {
        return SchemaType::Integer;
    }

    // Alias definitions: aliases.*
    if field.starts_with("aliases.") {
        return SchemaType::String;
    }

    // Environment variables: environment.*
    if field.starts_with("environment.") {
        return SchemaType::String;
    }

    SchemaType::Unknown
}

/// Parse a value according to its schema type
pub fn parse_value_with_schema(field: &str, values: &[String], global: bool) -> Result<Value> {
    let schema_type = lookup_field_type(field, global);

    match schema_type {
        SchemaType::Array { item_type } => {
            // For arrays, collect all values
            let items: Result<Vec<Value>> = values
                .iter()
                .map(|v| parse_scalar_value(v, &item_type))
                .collect();
            Ok(Value::Sequence(items?))
        }
        SchemaType::Object => {
            // For objects, we can't infer from a single value
            // Fall back to YAML parsing
            if values.len() == 1 {
                serde_yaml_ng::from_str(&values[0])
                    .or_else(|_| Ok(Value::String(values[0].clone())))
            } else {
                Err(VmError::Config(format!(
                    "Field '{}' is an object type. Use dot notation (e.g., '{}.key value') or YAML syntax",
                    field, field
                )))
            }
        }
        _ => {
            // For scalar types, only accept single value
            if values.len() > 1 {
                return Err(VmError::Config(format!(
                    "Field '{}' expects a single value, got {} values",
                    field,
                    values.len()
                )));
            }
            parse_scalar_value(&values[0], &schema_type)
        }
    }
}

/// Parse a scalar value according to its type
fn parse_scalar_value(value: &str, schema_type: &SchemaType) -> Result<Value> {
    match schema_type {
        SchemaType::Boolean => parse_boolean(value),
        SchemaType::Integer => parse_integer(value),
        SchemaType::String => Ok(Value::String(value.to_string())),
        SchemaType::Unknown => {
            // Try YAML parsing, fallback to string
            serde_yaml_ng::from_str(value).or_else(|_| Ok(Value::String(value.to_string())))
        }
        _ => Err(VmError::Config(format!(
            "Cannot parse '{}' as scalar type",
            value
        ))),
    }
}

/// Parse a boolean value from string
fn parse_boolean(value: &str) -> Result<Value> {
    match value.to_lowercase().as_str() {
        "true" | "yes" | "1" | "on" => Ok(Value::Bool(true)),
        "false" | "no" | "0" | "off" => Ok(Value::Bool(false)),
        _ => Err(VmError::Config(format!(
            "'{}' is not a valid boolean value. Use: true, false, yes, no, 1, 0, on, off",
            value
        ))),
    }
}

/// Parse an integer value from string
fn parse_integer(value: &str) -> Result<Value> {
    value
        .parse::<i64>()
        .map(|n| Value::Number(n.into()))
        .map_err(|_| VmError::Config(format!("'{}' is not a valid integer value", value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_array_fields() {
        assert_eq!(
            lookup_field_type("networking.networks", false),
            SchemaType::Array {
                item_type: Box::new(SchemaType::String)
            }
        );
        assert_eq!(
            lookup_field_type("apt_packages", false),
            SchemaType::Array {
                item_type: Box::new(SchemaType::String)
            }
        );
    }

    #[test]
    fn test_lookup_boolean_fields() {
        assert_eq!(
            lookup_field_type("services.postgresql.enabled", false),
            SchemaType::Boolean
        );
        assert_eq!(
            lookup_field_type("terminal.show_git_branch", false),
            SchemaType::Boolean
        );
    }

    #[test]
    fn test_lookup_integer_fields() {
        // vm.memory and vm.cpus are now String to support flexible formats (e.g., "1gb", "50%", "unlimited")
        assert_eq!(lookup_field_type("vm.memory", false), SchemaType::String);
        assert_eq!(lookup_field_type("vm.cpus", false), SchemaType::String);
        assert_eq!(
            lookup_field_type("ports.frontend", false),
            SchemaType::Integer
        );
    }

    #[test]
    fn test_parse_array_value() {
        let values = vec!["jules".to_string(), "docker-net".to_string()];
        let result = parse_value_with_schema("networking.networks", &values, false).unwrap();

        if let Value::Sequence(seq) = result {
            assert_eq!(seq.len(), 2);
            assert_eq!(seq[0], Value::String("jules".to_string()));
            assert_eq!(seq[1], Value::String("docker-net".to_string()));
        } else {
            panic!("Expected sequence");
        }
    }

    #[test]
    fn test_parse_single_array_value() {
        let values = vec!["jules".to_string()];
        let result = parse_value_with_schema("networking.networks", &values, false).unwrap();

        if let Value::Sequence(seq) = result {
            assert_eq!(seq.len(), 1);
            assert_eq!(seq[0], Value::String("jules".to_string()));
        } else {
            panic!("Expected sequence");
        }
    }

    #[test]
    fn test_parse_boolean_variants() {
        let test_cases = vec![
            ("true", true),
            ("false", false),
            ("yes", true),
            ("no", false),
            ("1", true),
            ("0", false),
            ("on", true),
            ("off", false),
        ];

        for (input, expected) in test_cases {
            let result = parse_boolean(input).unwrap();
            assert_eq!(result, Value::Bool(expected));
        }
    }

    #[test]
    fn test_parse_integer() {
        let result = parse_integer("8192").unwrap();
        if let Value::Number(n) = result {
            assert_eq!(n.as_i64(), Some(8192));
        } else {
            panic!("Expected number");
        }
    }
}
