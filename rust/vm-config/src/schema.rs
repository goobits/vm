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
static VM_SCHEMA_CACHE: Lazy<HashMap<String, SchemaType>> =
    Lazy::new(|| build_schema_cache(include_str!("../../../configs/schema/vm.schema.yaml")));
static GLOBAL_SCHEMA_CACHE: Lazy<HashMap<String, SchemaType>> =
    Lazy::new(|| build_schema_cache(include_str!("../../../configs/schema/global.schema.yaml")));

fn build_schema_cache(source: &str) -> HashMap<String, SchemaType> {
    let schema: Value =
        serde_yaml_ng::from_str(source).expect("embedded configuration schema must be valid YAML");
    let mut cache = HashMap::new();
    collect_children(&schema, &schema, "", &mut cache);
    cache
}

fn collect_schema(
    root: &Value,
    schema: &Value,
    path: &str,
    cache: &mut HashMap<String, SchemaType>,
) {
    let schema = resolve_reference(root, schema);
    cache.insert(path.to_string(), schema_type(root, schema));
    collect_children(root, schema, path, cache);
    for variant in sequence_field(schema, "oneOf") {
        collect_children(root, resolve_reference(root, variant), path, cache);
    }
}

fn collect_children(
    root: &Value,
    schema: &Value,
    path: &str,
    cache: &mut HashMap<String, SchemaType>,
) {
    let schema = resolve_reference(root, schema);
    if let Some(properties) = mapping_field(schema, "properties") {
        for (name, child) in properties {
            let Some(name) = name.as_str() else {
                continue;
            };
            collect_schema(root, child, &joined_path(path, name), cache);
        }
    }

    if let Some(additional) =
        value_field(schema, "additionalProperties").filter(|value| value.is_mapping())
    {
        collect_schema(root, additional, &joined_path(path, "*"), cache);
    }

    if string_field(schema, "type") == Some("array") {
        if let Some(items) = value_field(schema, "items").filter(|value| value.is_mapping()) {
            collect_schema(root, items, &joined_path(path, "*"), cache);
        }
    }
}

fn schema_type(root: &Value, schema: &Value) -> SchemaType {
    let schema = resolve_reference(root, schema);
    if let Some(kind) = string_field(schema, "type") {
        return match kind {
            "string" => SchemaType::String,
            "integer" | "number" => SchemaType::Integer,
            "boolean" => SchemaType::Boolean,
            "array" => SchemaType::Array {
                item_type: Box::new(
                    value_field(schema, "items")
                        .map_or(SchemaType::Unknown, |items| schema_type(root, items)),
                ),
            },
            "object" => SchemaType::Object,
            _ => SchemaType::Unknown,
        };
    }

    let variants = sequence_field(schema, "oneOf")
        .map(|variant| schema_type(root, variant))
        .collect::<Vec<_>>();
    for preferred in [
        SchemaType::String,
        SchemaType::Boolean,
        SchemaType::Integer,
        SchemaType::Object,
    ] {
        if variants.contains(&preferred) {
            return preferred;
        }
    }
    variants.into_iter().next().unwrap_or(SchemaType::Unknown)
}

fn resolve_reference<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let Some(reference) = string_field(schema, "$ref") else {
        return schema;
    };
    let Some(pointer) = reference.strip_prefix("#/") else {
        return schema;
    };
    pointer.split('/').fold(root, |current, segment| {
        value_field(current, &segment.replace("~1", "/").replace("~0", "~")).unwrap_or(schema)
    })
}

fn value_field<'a>(value: &'a Value, field: &str) -> Option<&'a Value> {
    value.as_mapping()?.get(Value::String(field.to_string()))
}

fn mapping_field<'a>(value: &'a Value, field: &str) -> Option<&'a serde_yaml_ng::Mapping> {
    value_field(value, field)?.as_mapping()
}

fn sequence_field<'a>(value: &'a Value, field: &str) -> impl Iterator<Item = &'a Value> {
    value_field(value, field)
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
}

fn string_field<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value_field(value, field)?.as_str()
}

fn joined_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
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

    let segments = field.split('.').collect::<Vec<_>>();
    cache
        .iter()
        .filter_map(|(pattern, schema_type)| {
            let pattern = pattern.split('.').collect::<Vec<_>>();
            (pattern.len() == segments.len()
                && pattern
                    .iter()
                    .zip(&segments)
                    .all(|(expected, actual)| *expected == "*" || expected == actual))
            .then_some((
                pattern.iter().filter(|segment| **segment != "*").count(),
                schema_type,
            ))
        })
        .max_by_key(|(specificity, _)| *specificity)
        .map_or(SchemaType::Unknown, |(_, schema_type)| schema_type.clone())
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
        SchemaType::Object => {
            let parsed: Value = serde_yaml_ng::from_str(value).map_err(|error| {
                VmError::Config(format!("'{value}' is not a valid object: {error}"))
            })?;
            if parsed.is_mapping() {
                Ok(parsed)
            } else {
                Err(VmError::Config(format!("'{value}' is not a valid object")))
            }
        }
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
    fn canonical_vm_schema_is_valid_yaml() {
        let schema: Value =
            serde_yaml_ng::from_str(include_str!("../../../configs/schema/vm.schema.yaml"))
                .unwrap();

        assert!(schema["properties"]["storage"].is_mapping());
        assert!(schema["properties"]["mounts"].is_mapping());
        assert!(schema["properties"]["tools"].is_mapping());
        assert!(schema["properties"]["vm"]["properties"]["pids_limit"].is_mapping());
    }

    #[test]
    fn canonical_global_schema_covers_managed_services() {
        let schema: Value =
            serde_yaml_ng::from_str(include_str!("../../../configs/schema/global.schema.yaml"))
                .unwrap();
        let services = &schema["properties"]["services"]["properties"];

        for name in ["auth_proxy", "postgresql", "redis", "mongodb", "mysql"] {
            assert!(services[name].is_mapping(), "missing global service {name}");
        }
        assert!(schema["properties"]["backups"].is_mapping());
        assert!(schema["properties"]["snapshots"].is_mapping());
        assert!(schema["properties"]["packages"]["properties"]["source_roots"].is_mapping());
        assert!(schema["properties"]["packages"]["properties"]["canonical_sources"].is_mapping());
    }

    #[test]
    fn parses_mount_objects_for_config_set() {
        let value = parse_value_with_schema(
            "mounts",
            &["{source: ../auth, target: /packages/auth, access: read_only}".to_string()],
            false,
        )
        .unwrap();

        assert_eq!(value[0]["source"], "../auth");
        assert_eq!(value[0]["access"], "read_only");
    }

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
        assert_eq!(
            lookup_field_type("bootstrap.playwright.browsers", false),
            SchemaType::Array {
                item_type: Box::new(SchemaType::String)
            }
        );
    }

    #[test]
    fn looks_up_global_package_source_roots() {
        for field in ["packages.source_roots", "packages.canonical_sources"] {
            assert_eq!(
                lookup_field_type(field, true),
                SchemaType::Array {
                    item_type: Box::new(SchemaType::String)
                }
            );
        }
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
        assert_eq!(
            lookup_field_type("host_sync.ai_tools.antigravity", false),
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
    fn test_lookup_storage_fields() {
        assert_eq!(
            lookup_field_type("storage.volumes.node_modules.nocopy", false),
            SchemaType::Boolean
        );
        assert_eq!(
            lookup_field_type("storage.volumes.pnpm_store.scope", false),
            SchemaType::String
        );
        assert_eq!(
            lookup_field_type("storage.tmpfs.0.mode", false),
            SchemaType::String
        );
        assert_eq!(
            lookup_field_type("storage.tmpfs", false),
            SchemaType::Array {
                item_type: Box::new(SchemaType::Object)
            }
        );
    }

    #[test]
    fn schema_cache_follows_references_and_additional_properties() {
        assert_eq!(
            lookup_field_type("services.postgresql.version", true),
            SchemaType::String
        );
        assert_eq!(
            lookup_field_type("tools.typemill.updates", true),
            SchemaType::String
        );
        assert_eq!(
            lookup_field_type("mounts.0.access", false),
            SchemaType::String
        );
    }

    #[test]
    fn looks_up_dynamic_tool_fields() {
        assert_eq!(
            lookup_field_type("tools.codex.version", false),
            SchemaType::String
        );
        assert_eq!(
            lookup_field_type("tools.agent-skills.updates", false),
            SchemaType::String
        );
        assert_eq!(lookup_field_type("tools.codex", false), SchemaType::Object);
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
