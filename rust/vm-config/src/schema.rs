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

/// Helper macro to add string fields to schema cache
macro_rules! add_strings {
    ($cache:expr, $($field:expr),+ $(,)?) => {
        $(
            $cache.insert($field.to_string(), SchemaType::String);
        )+
    };
}

/// Helper macro to add boolean fields to schema cache
macro_rules! add_booleans {
    ($cache:expr, $($field:expr),+ $(,)?) => {
        $(
            $cache.insert($field.to_string(), SchemaType::Boolean);
        )+
    };
}

/// Helper macro to add integer fields to schema cache
macro_rules! add_integers {
    ($cache:expr, $($field:expr),+ $(,)?) => {
        $(
            $cache.insert($field.to_string(), SchemaType::Integer);
        )+
    };
}

/// Helper macro to add string array fields to schema cache
macro_rules! add_string_arrays {
    ($cache:expr, $($field:expr),+ $(,)?) => {
        $(
            $cache.insert($field.to_string(), SchemaType::Array {
                item_type: Box::new(SchemaType::String),
            });
        )+
    };
}

/// Add common service fields (enabled and port) for multiple services
fn add_service_entries(cache: &mut HashMap<String, SchemaType>, services: &[&str]) {
    for service in services {
        cache.insert(format!("services.{}.enabled", service), SchemaType::Boolean);
        cache.insert(format!("services.{}.port", service), SchemaType::Integer);
    }
}

/// Add database service fields (user, password, database)
fn add_database_service_fields(cache: &mut HashMap<String, SchemaType>, service: &str) {
    let fields = ["user", "password", "database"];
    for field in fields {
        cache.insert(
            format!("services.{}.{}", service, field),
            SchemaType::String,
        );
    }
}

/// Build the VM schema cache from embedded schema
fn build_vm_schema_cache() -> HashMap<String, SchemaType> {
    let mut cache = HashMap::new();

    add_vm_schema_fields(&mut cache);
    add_service_schema_fields(&mut cache);
    add_terminal_and_package_fields(&mut cache);

    cache
}

/// Add VM-related schema fields
fn add_vm_schema_fields(cache: &mut HashMap<String, SchemaType>) {
    // Simple scalar types and project fields
    add_strings!(
        cache,
        "version",
        "os",
        "provider",
        "project.name",
        "project.hostname",
        "project.workspace_path",
        "project.env_template_path",
        "project.backup_pattern"
    );

    // VM fields
    add_strings!(
        cache,
        "vm.box",
        "vm.memory",
        "vm.cpus",
        "vm.user",
        "vm.port_binding",
        "vm.timezone",
        "vm.swap"
    );
    add_booleans!(cache, "vm.gui");
    add_integers!(cache, "vm.swappiness");

    // Version fields
    add_strings!(
        cache,
        "versions.node",
        "versions.nvm",
        "versions.pnpm",
        "versions.python"
    );

    // Port fields
    cache.insert(
        "ports._range".to_string(),
        SchemaType::Array {
            item_type: Box::new(SchemaType::Integer),
        },
    );

    // Tart fields
    add_strings!(
        cache,
        "tart.image",
        "tart.guest_os",
        "tart.disk_size",
        "tart.ssh_user",
        "tart.storage_path"
    );
    add_booleans!(cache, "tart.rosetta", "tart.install_docker");
}

/// Add service-related schema fields
fn add_service_schema_fields(cache: &mut HashMap<String, SchemaType>) {
    // Service fields (common pattern)
    add_service_entries(
        cache,
        &[
            "postgresql",
            "redis",
            "mongodb",
            "mysql",
            "docker",
            "headless_browser",
        ],
    );

    // Database-specific fields
    add_database_service_fields(cache, "postgresql");
    add_database_service_fields(cache, "mysql");

    // Simple boolean services (gpu/audio/video passthrough)
    add_booleans!(cache, "services.gpu", "services.audio", "services.video");

    // Service-specific fields
    add_booleans!(cache, "services.docker.buildx");
    add_strings!(
        cache,
        "services.headless_browser.display",
        "services.headless_browser.executable_path"
    );
}

/// Add terminal, package, and host sync schema fields
fn add_terminal_and_package_fields(cache: &mut HashMap<String, SchemaType>) {
    // Terminal fields
    add_strings!(
        cache,
        "terminal.emoji",
        "terminal.username",
        "terminal.theme"
    );
    add_booleans!(cache, "terminal.show_git_branch", "terminal.show_timestamp");

    // Package arrays
    add_string_arrays!(
        cache,
        "apt_packages",
        "npm_packages",
        "cargo_packages",
        "pip_packages"
    );

    // Object types
    cache.insert("aliases".to_string(), SchemaType::Object);
    cache.insert("environment".to_string(), SchemaType::Object);

    // Host synchronization
    add_booleans!(
        cache,
        "host_sync.git_config",
        "host_sync.ssh_agent",
        "host_sync.ssh_config",
        "host_sync.ai_tools",
        "host_sync.package_links.npm",
        "host_sync.package_links.pip",
        "host_sync.package_links.cargo",
        "host_sync.worktrees.enabled"
    );
    add_strings!(cache, "host_sync.worktrees.base_path");
    add_string_arrays!(cache, "host_sync.dotfiles", "networking.networks");
}

/// Build the global schema cache
fn build_global_schema_cache() -> HashMap<String, SchemaType> {
    let mut cache = HashMap::new();

    // Docker registry service
    add_booleans!(
        cache,
        "services.docker_registry.enabled",
        "services.docker_registry.enable_lru_eviction",
        "services.docker_registry.enable_auto_restart"
    );
    add_integers!(
        cache,
        "services.docker_registry.port",
        "services.docker_registry.max_cache_size_gb",
        "services.docker_registry.max_image_age_days",
        "services.docker_registry.cleanup_interval_hours",
        "services.docker_registry.health_check_interval_minutes"
    );

    // Auth proxy service
    add_booleans!(cache, "services.auth_proxy.enabled");
    add_integers!(
        cache,
        "services.auth_proxy.port",
        "services.auth_proxy.token_expiry_hours"
    );

    // Package registry service
    add_booleans!(cache, "services.package_registry.enabled");
    add_integers!(
        cache,
        "services.package_registry.port",
        "services.package_registry.max_storage_gb"
    );

    // Defaults
    add_strings!(
        cache,
        "defaults.provider",
        "defaults.user",
        "defaults.terminal.theme",
        "defaults.terminal.shell"
    );
    add_integers!(cache, "defaults.memory", "defaults.cpus");

    // Features
    add_booleans!(
        cache,
        "features.auto_detect_presets",
        "features.auto_port_allocation",
        "features.telemetry",
        "features.update_notifications"
    );

    // Worktrees
    add_booleans!(cache, "worktrees.enabled");
    add_strings!(cache, "worktrees.base_path");

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
