use crate::config::VmConfig;
use serde_json::Value;
use vm_core::error::Result;

/// Deep merge strategy for VM configurations.
///
/// Provides sophisticated configuration merging capabilities that intelligently
/// combine multiple configuration sources while preserving the intended behavior
/// for different data types.
///
/// ## Merge Strategy
/// - **Objects**: Deep merge, with overlay values taking precedence
/// - **Arrays**: Complete replacement (overlay replaces base entirely)
/// - **Primitives**: Simple replacement
///
/// This strategy is designed for configuration use cases where:
/// - Object merging allows for additive configuration
/// - Array replacement prevents unexpected behavior from partial merges
/// - Primitive replacement provides predictable overrides
///
/// # Examples
/// ```rust
/// use vm_config::merge::ConfigMerger;
/// use vm_config::config::VmConfig;
///
/// let base = VmConfig::default();
/// let overlay = VmConfig::default();
///
/// let merger = ConfigMerger::new(base);
/// let result = merger.merge(overlay)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct ConfigMerger {
    base: VmConfig,
}

impl ConfigMerger {
    /// Create a new configuration merger with a base configuration.
    ///
    /// The base configuration serves as the foundation for all merge operations.
    /// Subsequent overlays will be merged on top of this base.
    ///
    /// # Arguments
    /// * `base` - The base configuration to start with
    ///
    /// # Returns
    /// A new `ConfigMerger` instance ready for merge operations
    pub fn new(base: VmConfig) -> Self {
        Self { base }
    }

    /// Merge another configuration into the base, with overlay taking precedence.
    ///
    /// Performs a deep merge operation where the overlay configuration values
    /// override corresponding values in the base configuration. The merge behavior
    /// depends on the data type:
    ///
    /// - **Objects**: Recursively merged (additive)
    /// - **Arrays**: Completely replaced (not merged)
    /// - **Primitives**: Simply replaced
    ///
    /// # Arguments
    /// * `overlay` - The configuration to merge on top of the base
    ///
    /// # Returns
    /// A new `VmConfig` with the merged result
    ///
    /// # Errors
    /// Returns an error if JSON serialization/deserialization fails during merging
    ///
    /// # Examples
    /// ```rust
    /// use vm_config::merge::ConfigMerger;
    /// use vm_config::config::VmConfig;
    ///
    /// let base = VmConfig::default();
    /// let overlay = VmConfig::default();
    ///
    /// let result = ConfigMerger::new(base).merge(overlay)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn merge(self, overlay: VmConfig) -> Result<VmConfig> {
        // Convert to JSON values for deep merging
        let mut base_value = serde_json::to_value(&self.base)?;
        let overlay_value = serde_json::to_value(&overlay)?;

        deep_merge(&mut base_value, overlay_value);

        // Convert back to VmConfig
        Ok(serde_json::from_value(base_value)?)
    }

    /// Merge multiple configurations in order.
    ///
    /// Applies multiple overlay configurations sequentially, where each overlay
    /// is merged on top of the previous result. This is useful for building
    /// configuration hierarchies like: defaults → global → preset → user.
    ///
    /// # Arguments
    /// * `overlays` - Vector of configurations to merge in order
    ///
    /// # Returns
    /// A new `VmConfig` with all overlays applied sequentially
    ///
    /// # Errors
    /// Returns an error if any individual merge operation fails
    ///
    /// # Examples
    /// ```rust
    /// use vm_config::merge::ConfigMerger;
    /// use vm_config::config::VmConfig;
    ///
    /// let base = VmConfig::default();
    /// let overlays = vec![VmConfig::default(), VmConfig::default()];
    ///
    /// let result = ConfigMerger::new(base).merge_all(overlays)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn merge_all(self, overlays: Vec<VmConfig>) -> Result<VmConfig> {
        let mut result = self.base;
        for overlay in overlays {
            result = ConfigMerger::new(result).merge(overlay)?;
        }
        Ok(result)
    }
}

/// Deep merge JSON values recursively
fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(&key) {
                    Some(base_value) => {
                        // Special handling for arrays - replace don't merge
                        if matches!(overlay_value, Value::Array(_)) {
                            base_map.insert(key, overlay_value);
                        } else {
                            deep_merge(base_value, overlay_value);
                        }
                    }
                    None => {
                        base_map.insert(key, overlay_value);
                    }
                }
            }
        }
        (base_val, overlay_val) => {
            // For non-objects, overlay completely replaces base
            *base_val = overlay_val;
        }
    }
}

/// Merge configurations following VM tool precedence rules.
///
/// This is the main configuration merging function that implements the VM tool's
/// configuration hierarchy. Configurations are merged in order of precedence:
///
/// 1. **Default config** - Base VM tool defaults
/// 2. **Global config** - User's global settings (~/.config/vm/global.yaml)
/// 3. **Preset config** - Auto-detected or specified preset
/// 4. **User config** - Project-specific configuration (highest priority)
///
/// Each layer can be `None`, in which case it's skipped. The result is a
/// fully merged configuration ready for use.
///
/// # Arguments
/// * `default` - Base default configuration (typically from VM tool)
/// * `global` - User's global configuration file
/// * `preset` - Auto-detected or manually specified preset
/// * `user` - Project-specific user configuration
///
/// # Returns
/// A fully merged `VmConfig` with all applicable configurations applied
///
/// # Errors
/// Returns an error if any merge operation fails
///
/// # Examples
/// ```rust
/// use vm_config::merge::merge_configs;
/// use vm_config::config::VmConfig;
///
/// let default_config = Some(VmConfig::default());
/// let user_config = Some(VmConfig::default());
///
/// let result = merge_configs(default_config, None, None, user_config)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn merge_configs(
    default: Option<VmConfig>,
    global: Option<VmConfig>,
    preset: Option<VmConfig>,
    user: Option<VmConfig>,
) -> Result<VmConfig> {
    let base = default.unwrap_or_default();
    let merger = ConfigMerger::new(base);

    let mut overlays = Vec::new();
    if let Some(g) = global {
        overlays.push(g);
    }
    if let Some(p) = preset {
        overlays.push(p);
    }
    if let Some(u) = user {
        overlays.push(u);
    }

    merger.merge_all(overlays)
}

/// Apply a named profile to the base configuration.
///
/// Looks up the specified profile name in the `profiles` map of the base configuration
/// and merges it on top of the base. If the profile is not found, returns an error.
///
/// # Arguments
/// * `base` - The base configuration containing the profiles
/// * `profile_name` - The name of the profile to apply
///
/// # Returns
/// A new `VmConfig` with the profile applied
pub fn apply_profile(base: VmConfig, profile_name: &str) -> Result<VmConfig> {
    let profile = base
        .profiles
        .as_ref()
        .and_then(|p| p.get(profile_name))
        .cloned();

    match profile {
        Some(profile_config) => {
            // Create a merger with the base config and merge the profile on top
            ConfigMerger::new(base).merge(profile_config)
        }
        None => Err(vm_core::error::VmError::Config(format!(
            "Profile '{}' not found in configuration",
            profile_name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deep_merge() {
        let mut base = json!({
            "project": {
                "name": "base",
                "hostname": "base.local"
            },
            "vm": {
                "memory": 2048
            },
            "services": {
                "docker": {
                    "enabled": true
                }
            }
        });

        let overlay = json!({
            "project": {
                "name": "overlay"
            },
            "vm": {
                "cpus": 4
            },
            "services": {
                "redis": {
                    "enabled": true
                }
            }
        });

        deep_merge(&mut base, overlay);

        assert_eq!(base["project"]["name"], "overlay");
        assert_eq!(base["project"]["hostname"], "base.local");
        assert_eq!(base["vm"]["memory"], 2048);
        assert_eq!(base["vm"]["cpus"], 4);
        assert_eq!(base["services"]["docker"]["enabled"], true);
        assert_eq!(base["services"]["redis"]["enabled"], true);
    }

    #[test]
    fn test_array_replacement_behavior() {
        // Test that arrays replace rather than merge (intentional behavior)
        let mut base = json!({
            "npm_packages": ["eslint", "typescript"],
            "ports": {"web": 3000, "api": 4000},
            "services": {
                "redis": {"enabled": true}
            }
        });

        let overlay = json!({
            "npm_packages": ["prettier", "jest"],  // Should completely replace
            "ports": {"web": 8080},  // Should merge (not array)
            "services": {
                "redis": {"port": 6379}  // Should merge nested object
            }
        });

        deep_merge(&mut base, overlay);

        // Array should be completely replaced, not merged
        assert_eq!(base["npm_packages"], json!(["prettier", "jest"]));
        assert!(!base["npm_packages"]
            .as_array()
            .expect("should be an array")
            .contains(&json!("eslint")));
        assert!(!base["npm_packages"]
            .as_array()
            .expect("should be an array")
            .contains(&json!("typescript")));

        // Objects should merge
        assert_eq!(base["ports"]["web"], 8080); // Updated
        assert_eq!(base["ports"]["api"], 4000); // Preserved
        assert_eq!(base["services"]["redis"]["enabled"], true); // Preserved
        assert_eq!(base["services"]["redis"]["port"], 6379); // Added
    }

    #[test]
    fn test_empty_array_edge_cases() {
        // Test edge cases with empty arrays and null values
        let mut base = json!({
            "full_array": ["item1", "item2"],
            "empty_array": [],
            "null_field": null,
            "string_field": "value"
        });

        let overlay = json!({
            "full_array": [],           // Empty array should replace full array
            "empty_array": ["new_item"], // Non-empty should replace empty
            "null_field": ["from_null"], // Array should replace null
            "string_field": []          // Empty array should replace string
        });

        deep_merge(&mut base, overlay);

        assert_eq!(base["full_array"], json!([]));
        assert_eq!(base["empty_array"], json!(["new_item"]));
        assert_eq!(base["null_field"], json!(["from_null"]));
        assert_eq!(base["string_field"], json!([]));
    }

    #[test]
    fn test_mixed_type_replacement() {
        // Test that arrays can replace any type and vice versa
        let mut base = json!({
            "array_to_object": ["item1", "item2"],
            "object_to_array": {"key": "value"},
            "number_to_array": 42,
            "array_to_string": ["old"],
            "nested": {
                "array_field": ["nested_item"]
            }
        });

        let overlay = json!({
            "array_to_object": {"new": "object"},
            "object_to_array": ["new", "array"],
            "number_to_array": ["from_number"],
            "array_to_string": "now_string",
            "nested": {
                "array_field": {"converted": "to_object"}
            }
        });

        deep_merge(&mut base, overlay);

        // Verify type conversions
        assert!(base["array_to_object"].is_object());
        assert_eq!(base["array_to_object"]["new"], "object");

        assert!(base["object_to_array"].is_array());
        assert_eq!(base["object_to_array"], json!(["new", "array"]));

        assert!(base["number_to_array"].is_array());
        assert_eq!(base["number_to_array"], json!(["from_number"]));

        assert!(base["array_to_string"].is_string());
        assert_eq!(base["array_to_string"], "now_string");

        // Nested conversions should work too
        assert!(base["nested"]["array_field"].is_object());
        assert_eq!(base["nested"]["array_field"]["converted"], "to_object");
    }

    #[test]
    fn test_production_config_precedence_with_arrays() {
        // Test realistic config scenario: defaults → global → preset → user
        let mut defaults = json!({
            "npm_packages": ["basic-tools"],
            "services": {"docker": {"enabled": false}},
            "vm": {"memory": 1024}
        });

        // Global config adds more packages
        let global = json!({
            "npm_packages": ["basic-tools", "global-linting"],
            "vm": {"memory": 2048}
        });
        deep_merge(&mut defaults, global);

        // Preset completely overrides packages (typical use case)
        let preset = json!({
            "npm_packages": ["react-preset", "typescript"],
            "services": {"redis": {"enabled": true}}
        });
        deep_merge(&mut defaults, preset);

        // User adds their own packages (final override)
        let user = json!({
            "npm_packages": ["my-custom-tools"],
            "vm": {"cpus": 4}
        });
        deep_merge(&mut defaults, user);

        // Final result should have only user's packages (array replacement)
        assert_eq!(defaults["npm_packages"], json!(["my-custom-tools"]));
        assert!(!defaults["npm_packages"]
            .as_array()
            .expect("should be an array")
            .contains(&json!("basic-tools")));
        assert!(!defaults["npm_packages"]
            .as_array()
            .expect("should be an array")
            .contains(&json!("react-preset")));

        // But other fields should be properly merged
        assert_eq!(defaults["vm"]["memory"], 2048); // From global
        assert_eq!(defaults["vm"]["cpus"], 4); // From user
        assert_eq!(defaults["services"]["docker"]["enabled"], false); // From defaults
        assert_eq!(defaults["services"]["redis"]["enabled"], true); // From preset
    }
}
