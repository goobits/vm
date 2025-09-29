//! YAML formatting and organization utilities
//!
//! This module ensures consistent formatting and field ordering for all YAML output.

use indexmap::IndexMap;
use serde_yaml::{Mapping, Value};
use serde_yaml_ng as serde_yaml;
use vm_core::error::Result;

/// Canonical field order for VM configuration YAML files.
/// This matches the logical sections in the VmConfig struct.
const FIELD_ORDER: &[&str] = &[
    // 1. Metadata & Schema
    "$schema",
    "version",
    // 2. Provider & Environment
    "provider",
    "os",
    "tart",
    // 3. Project Identity
    "project",
    // 4. VM Resources
    "vm",
    // 5. Runtime Versions
    "versions",
    // 6. Networking
    "ports",
    // 7. Services & Infrastructure
    "services",
    // 8. Package Management
    "apt_packages",
    "npm_packages",
    "pip_packages",
    "cargo_packages",
    "package_linking",
    // 9. Development Environment
    "terminal",
    "aliases",
    "environment",
    // 10. Feature Flags & Integrations
    "claude_sync",
    "gemini_sync",
    "persist_databases",
    // 11. Security
    "security",
];

/// Format and organize a YAML Value according to the canonical field order.
///
/// This function:
/// - Reorders top-level fields according to FIELD_ORDER
/// - Preserves any fields not in FIELD_ORDER at the end
/// - Maintains nested structure ordering where appropriate
/// - Ensures consistent formatting
pub fn format_yaml_value(value: &Value) -> Result<Value> {
    match value {
        Value::Mapping(map) => {
            let mut ordered_map = IndexMap::new();

            // First, add fields in canonical order
            for field_name in FIELD_ORDER {
                if let Some(key) = map.keys().find(|k| k.as_str() == Some(*field_name)) {
                    if let Some(val) = map.get(key) {
                        // Recursively format nested mappings
                        let formatted_val = format_field_value(field_name, val)?;
                        ordered_map.insert(key.clone(), formatted_val);
                    }
                }
            }

            // Then add any remaining fields not in FIELD_ORDER
            for (key, val) in map.iter() {
                if let Some(key_str) = key.as_str() {
                    if !FIELD_ORDER.contains(&key_str) && !ordered_map.contains_key(key) {
                        ordered_map.insert(key.clone(), val.clone());
                    }
                }
            }

            // Convert IndexMap to serde_yaml Mapping
            let mut result = Mapping::new();
            for (k, v) in ordered_map {
                result.insert(k, v);
            }
            Ok(Value::Mapping(result))
        }
        _ => Ok(value.clone()),
    }
}

/// Format a field value based on the field name.
fn format_field_value(field_name: &str, value: &Value) -> Result<Value> {
    if should_format_nested(field_name) {
        format_nested_value(value)
    } else {
        Ok(value.clone())
    }
}

/// Determine if a field's nested values should be formatted.
fn should_format_nested(field_name: &str) -> bool {
    matches!(
        field_name,
        "project" | "vm" | "versions" | "services" | "terminal" | "security" | "tart"
    )
}

/// Format nested mappings within specific configuration sections.
fn format_nested_value(value: &Value) -> Result<Value> {
    match value {
        Value::Mapping(map) => {
            // For nested mappings, maintain alphabetical order for consistency
            let mut sorted_entries: Vec<_> = map.iter().collect();
            sorted_entries.sort_by_key(|(k, _)| k.as_str().unwrap_or(""));

            let mut result = Mapping::new();
            for (k, v) in sorted_entries {
                // Recursively format deeper nested values
                let formatted = if matches!(v, Value::Mapping(_)) {
                    format_nested_value(v)?
                } else {
                    v.clone()
                };
                result.insert(k.clone(), formatted);
            }
            Ok(Value::Mapping(result))
        }
        _ => Ok(value.clone()),
    }
}

/// Post-process YAML string to ensure consistent formatting.
///
/// This handles:
/// - Consistent 2-space indentation
/// - Proper array formatting
/// - Clean line breaks
/// - Inline formatting for _range arrays
pub fn post_process_yaml(yaml: &str) -> String {
    let mut result = String::with_capacity(yaml.len());
    let mut in_array_section = false;
    let lines: Vec<&str> = yaml.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Check for _range array pattern to convert to inline format
        if trimmed == "_range:" && i + 2 < lines.len() {
            let next_line = lines[i + 1].trim();
            let third_line = lines[i + 2].trim();

            // Check if next two lines are array items with numbers
            if next_line.starts_with("- ") && third_line.starts_with("- ") {
                if let (Ok(first_val), Ok(second_val)) = (
                    next_line[2..].trim().parse::<u16>(),
                    third_line[2..].trim().parse::<u16>(),
                ) {
                    // Get the indentation from the original _range: line
                    let indent = &line[..line.len() - trimmed.len()];
                    result.push_str(&format!(
                        "{}_range: [{}, {}]\n",
                        indent, first_val, second_val
                    ));
                    i += 3; // Skip the next two lines
                    continue;
                }
            }
        }

        // Detect array sections
        if trimmed.ends_with("_packages:") || trimmed == "aliases:" || trimmed == "environment:" {
            in_array_section = true;
            result.push_str(line);
            result.push('\n');
            i += 1;
            continue;
        } else if !trimmed.is_empty() && trimmed.ends_with(':') && !trimmed.starts_with('-') {
            // New section, reset array flag
            in_array_section = false;
        }

        // Ensure array items have consistent indentation
        if in_array_section && trimmed.starts_with('-') {
            // Calculate proper indentation based on parent
            let indent = if line.starts_with("  -") {
                "  "
            } else if line.starts_with("    -") {
                "    "
            } else {
                "  " // Default to 2 spaces
            };
            result.push_str(indent);
            result.push_str(trimmed);
        } else {
            result.push_str(line);
        }
        result.push('\n');
        i += 1;
    }

    // Remove trailing newline if present (we'll add one in write)
    if result.ends_with("\n\n") {
        result.truncate(result.len() - 1);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_yaml_value_ordering() {
        let mut input = Mapping::new();
        input.insert(Value::String("vm".into()), Value::String("test".into()));
        input.insert(Value::String("version".into()), Value::String("1.0".into()));
        input.insert(
            Value::String("provider".into()),
            Value::String("docker".into()),
        );

        let result = format_yaml_value(&Value::Mapping(input)).unwrap();

        if let Value::Mapping(map) = result {
            let keys: Vec<_> = map.keys().filter_map(|k| k.as_str()).collect();
            assert_eq!(keys, vec!["version", "provider", "vm"]);
        } else {
            panic!("Expected Mapping, but got: {:?}", result);
        }
    }

    #[test]
    fn test_preserves_unknown_fields() {
        let mut input = Mapping::new();
        input.insert(
            Value::String("custom_field".into()),
            Value::String("value".into()),
        );
        input.insert(Value::String("version".into()), Value::String("1.0".into()));

        let result = format_yaml_value(&Value::Mapping(input)).unwrap();

        if let Value::Mapping(map) = result {
            assert!(map.contains_key("custom_field"));
            assert!(map.contains_key("version"));
        } else {
            panic!("Expected Mapping, but got: {:?}", result);
        }
    }

    #[test]
    fn test_range_inline_formatting() {
        let input = "_range:\n- 3300\n- 3309\n";
        let result = post_process_yaml(input);
        assert_eq!(result, "_range: [3300, 3309]\n");
    }

    #[test]
    fn test_range_inline_formatting_with_indentation() {
        let input = "ports:\n  _range:\n  - 3300\n  - 3309\n  frontend: 3301\n";
        let result = post_process_yaml(input);
        assert!(result.contains("  _range: [3300, 3309]"));
        assert!(result.contains("  frontend: 3301"));
    }

    #[test]
    fn test_range_inline_formatting_preserves_other_arrays() {
        let input = "npm_packages:\n- prettier\n- eslint\nports:\n  _range:\n  - 3300\n  - 3309\n";
        let result = post_process_yaml(input);
        assert!(result.contains("npm_packages:\n  - prettier\n  - eslint"));
        assert!(result.contains("  _range: [3300, 3309]"));
    }

    #[test]
    fn test_range_non_numeric_values_ignored() {
        let input = "_range:\n- start\n- end\n";
        let result = post_process_yaml(input);
        // Should not be converted to inline since values aren't numeric
        assert_eq!(result, "_range:\n- start\n- end\n");
    }
}
