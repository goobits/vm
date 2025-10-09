// Standard library
use std::sync::OnceLock;

// External crates
use regex::Regex;
use serde_yaml_ng as serde_yaml;
use serde_yaml_ng::Value;

// Internal imports
use crate::config::VmConfig;
use crate::ports::PortRange;
use crate::preset::PresetDetector;
use vm_core::error::Result;
use vm_core::error::VmError;
use vm_core::vm_warning;

static PORT_PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();

fn get_port_placeholder_regex() -> &'static Regex {
    PORT_PLACEHOLDER_RE.get_or_init(|| {
        Regex::new(r"\$\{port\.(\d+)\}").unwrap_or_else(|_| {
            Regex::new(r"never_matches_anything_specific_placeholder_12345").unwrap_or_else(|_| {
                Regex::new("").unwrap_or_else(|_| {
                    panic!("Critical: Even empty regex pattern is failing - regex engine corrupted")
                })
            })
        })
    })
}

pub fn replace_port_placeholders(value: &mut Value, port_range_str: &Option<String>) {
    let Some(port_range_str) = port_range_str.as_ref() else {
        return;
    };

    let port_range = match PortRange::parse(port_range_str) {
        Ok(range) => range,
        Err(_) => {
            vm_warning!("Could not parse port_range '{}'", port_range_str);
            return;
        }
    };

    replace_placeholders_recursive(value, &port_range);
}

fn extract_port_from_placeholder(s: &str, port_range: &PortRange) -> Option<u16> {
    let captures = get_port_placeholder_regex().captures(s)?;
    let index_match = captures.get(1)?;
    let index = index_match.as_str().parse::<u16>().ok()?;

    if index >= port_range.size() {
        vm_warning!(
            "Port index {} is out of bounds for the allocated range",
            index
        );
        return None;
    }

    Some(port_range.start + index)
}

fn replace_placeholders_recursive(value: &mut Value, port_range: &PortRange) {
    match value {
        Value::Mapping(mapping) => {
            for (_, val) in mapping.iter_mut() {
                replace_placeholders_recursive(val, port_range);
            }
        }
        Value::Sequence(sequence) => {
            for val in sequence.iter_mut() {
                replace_placeholders_recursive(val, port_range);
            }
        }
        Value::String(s) => {
            if let Some(port_value) = extract_port_from_placeholder(s, port_range) {
                *value = Value::Number(port_value.into());
            }
        }
        _ => {}
    }
}

/// Optimized preset loading with placeholder replacement
pub(crate) fn load_preset_with_placeholders(
    detector: &PresetDetector,
    preset_name: &str,
    port_range_str: &Option<String>,
) -> Result<VmConfig> {
    let raw_content = {
        if let Ok(plugins) = vm_plugin::discover_plugins() {
            let plugin_preset = plugins.iter().find(|p| {
                p.info.plugin_type == vm_plugin::PluginType::Preset && p.info.name == preset_name
            });

            if plugin_preset.is_some() {
                let original_config = detector.load_preset(preset_name)?;
                let Some(port_range_str) = port_range_str else {
                    return Ok(original_config);
                };
                let mut preset_value = serde_yaml::to_value(&original_config)?;
                replace_port_placeholders(&mut preset_value, &Some(port_range_str.clone()));
                return Ok(serde_yaml::from_value(preset_value)?);
            }
        }

        if let Some(content) = crate::embedded_presets::get_preset_content(preset_name) {
            content.to_string()
        } else {
            let original_config = detector.load_preset(preset_name)?;

            let Some(port_range_str) = port_range_str else {
                return Ok(original_config);
            };

            let mut preset_value = serde_yaml::to_value(&original_config)?;
            replace_port_placeholders(&mut preset_value, &Some(port_range_str.clone()));
            return Ok(serde_yaml::from_value(preset_value)?);
        }
    };

    let processed_content = if let Some(port_range_str) = port_range_str {
        replace_placeholders_in_string(&raw_content, port_range_str)?
    } else {
        raw_content
    };

    let preset_file: crate::preset::PresetFile = serde_yaml::from_str(&processed_content)
        .map_err(|e| VmError::Config(format!("Failed to parse preset '{}': {}", preset_name, e)))?;

    Ok(preset_file.config)
}

fn replace_placeholders_in_string(content: &str, port_range_str: &str) -> Result<String> {
    let port_range = match PortRange::parse(port_range_str) {
        Ok(range) => range,
        Err(_) => {
            vm_warning!("Could not parse port_range '{}'", port_range_str);
            return Ok(content.to_owned());
        }
    };

    let mut result = content.to_string();
    let regex = get_port_placeholder_regex();
    let mut replacements = Vec::new();

    for capture in regex.captures_iter(content) {
        if let (Some(full_match), Some(index_match)) = (capture.get(0), capture.get(1)) {
            if let Ok(index) = index_match.as_str().parse::<u16>() {
                if index < port_range.size() {
                    let port_value = port_range.start + index;
                    replacements.push((full_match.as_str().to_string(), port_value.to_string()));
                } else {
                    vm_warning!(
                        "Port index {} is out of bounds for the allocated range",
                        index
                    );
                }
            }
        }
    }

    for (placeholder, replacement) in replacements {
        result = result.replace(&placeholder, &replacement);
    }

    Ok(result)
}
