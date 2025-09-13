use crate::config::VmConfig;
use anyhow::Result;
use serde_json::Value;

/// Deep merge strategy for VM configurations
pub struct ConfigMerger {
    base: VmConfig,
}

impl ConfigMerger {
    pub fn new(base: VmConfig) -> Self {
        Self { base }
    }

    /// Merge another config into the base, with overlay taking precedence
    pub fn merge(mut self, overlay: VmConfig) -> Result<VmConfig> {
        // Convert to JSON values for deep merging
        let mut base_value = serde_json::to_value(&self.base)?;
        let overlay_value = serde_json::to_value(&overlay)?;

        deep_merge(&mut base_value, overlay_value);

        // Convert back to VmConfig
        Ok(serde_json::from_value(base_value)?)
    }

    /// Merge multiple configs in order
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

/// Merge configs following the VM tool precedence rules:
/// 1. Default config (base)
/// 2. Preset config (if detected)
/// 3. User config (highest priority)
pub fn merge_configs(
    default: Option<VmConfig>,
    preset: Option<VmConfig>,
    user: Option<VmConfig>,
) -> Result<VmConfig> {
    let base = default.unwrap_or_default();
    let mut merger = ConfigMerger::new(base);

    let mut overlays = Vec::new();
    if let Some(p) = preset {
        overlays.push(p);
    }
    if let Some(u) = user {
        overlays.push(u);
    }

    merger.merge_all(overlays)
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
}