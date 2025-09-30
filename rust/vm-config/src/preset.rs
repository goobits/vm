use crate::config::VmConfig;
use crate::detector::detect_preset_for_project;
use glob::glob;
use serde::{Deserialize, Serialize};
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;
use vm_core::error::{Result, VmError};
use vm_core::vm_error;

/// Metadata about a preset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetMetadata {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection: Option<serde_yaml::Value>,
}

/// Preset file structure with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetFile {
    pub preset: PresetMetadata,
    #[serde(flatten)]
    pub config: VmConfig,
}

/// Preset detection and loading
pub struct PresetDetector {
    project_dir: PathBuf,
    presets_dir: PathBuf,
}

impl PresetDetector {
    pub fn new(project_dir: PathBuf, presets_dir: PathBuf) -> Self {
        Self {
            project_dir,
            presets_dir,
        }
    }

    /// Detect the appropriate preset based on project files
    pub fn detect(&self) -> Option<String> {
        // Use vm-detector's comprehensive detection logic
        detect_preset_for_project(&self.project_dir)
    }

    /// Load a preset configuration by name
    pub fn load_preset(&self, name: &str) -> Result<VmConfig> {
        // Try embedded presets first
        if let Some(content) = crate::embedded_presets::get_preset_content(name) {
            let preset_file: PresetFile = serde_yaml::from_str(content).map_err(|e| {
                VmError::Serialization(format!("Failed to parse embedded preset '{}': {}", name, e))
            })?;
            return Ok(preset_file.config);
        }

        // Try plugin presets second
        if let Some(config) = self.load_plugin_preset(name)? {
            return Ok(config);
        }

        // Fallback to file system (for custom presets)
        let preset_path = self.presets_dir.join(format!("{}.yaml", name));
        if !preset_path.exists() {
            vm_error!(
                "Preset '{}' not found (not embedded and no file at {:?})",
                name,
                preset_path
            );
            return Err(VmError::Config("Preset not found".to_string()));
        }

        let content = std::fs::read_to_string(&preset_path)?;
        let preset_file: PresetFile = serde_yaml::from_str(&content).map_err(|e| {
            VmError::Serialization(format!("Failed to parse preset '{}': {}", name, e))
        })?;

        Ok(preset_file.config)
    }

    /// Load a preset from plugins
    fn load_plugin_preset(&self, name: &str) -> Result<Option<VmConfig>> {
        let discovery = match vm_plugin::PluginDiscovery::new() {
            Ok(d) => d,
            Err(_) => return Ok(None), // No plugins available
        };

        if let Some((_, plugin_preset)) = discovery.get_preset(name) {
            // Convert plugin preset to VmConfig
            // Note: Plugin presets provide basic configuration that gets merged
            // with other config sources during the normal merge process

            // Convert HashMap to IndexMap for environment
            let environment: indexmap::IndexMap<String, String> = plugin_preset
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            // Convert services list to ServiceConfig map
            let mut services = indexmap::IndexMap::new();
            for service_name in &plugin_preset.services {
                use crate::config::ServiceConfig;
                services.insert(
                    service_name.clone(),
                    ServiceConfig {
                        enabled: true,
                        ..Default::default()
                    },
                );
            }

            let mut config = VmConfig {
                apt_packages: plugin_preset.packages.clone(),
                environment,
                services,
                ..Default::default()
            };

            // Store base_image, ports, volumes, provision in extra_config for now
            // These can be used by provider-specific handling if needed
            use serde_json::json;
            if !plugin_preset.base_image.is_empty() {
                config.extra_config.insert(
                    "plugin_base_image".to_string(),
                    json!(plugin_preset.base_image),
                );
            }
            if !plugin_preset.ports.is_empty() {
                config
                    .extra_config
                    .insert("plugin_ports".to_string(), json!(plugin_preset.ports));
            }
            if !plugin_preset.volumes.is_empty() {
                config
                    .extra_config
                    .insert("plugin_volumes".to_string(), json!(plugin_preset.volumes));
            }
            if !plugin_preset.provision.is_empty() {
                config.extra_config.insert(
                    "plugin_provision".to_string(),
                    json!(plugin_preset.provision),
                );
            }

            return Ok(Some(config));
        }

        Ok(None)
    }

    /// Get list of available presets
    pub fn list_presets(&self) -> Result<Vec<String>> {
        let mut presets = Vec::new();

        // Add embedded presets
        for name in crate::embedded_presets::get_preset_names() {
            presets.push(name.to_string());
        }

        // Add plugin presets
        if let Ok(plugin_presets) = self.get_plugin_presets() {
            for name in plugin_presets {
                if !presets.contains(&name) {
                    presets.push(name);
                }
            }
        }

        // Add file system presets (if presets dir exists)
        if self.presets_dir.exists() {
            let pattern = self
                .presets_dir
                .join("*.yaml")
                .to_string_lossy()
                .to_string();
            for path in glob(&pattern)
                .map_err(|e| VmError::Filesystem(format!("Glob pattern error: {}", e)))?
                .flatten()
            {
                let Some(stem) = path.file_stem() else {
                    continue;
                };

                let name = stem.to_string_lossy().to_string();
                if !presets.contains(&name) {
                    presets.push(name);
                }
            }
        }

        presets.sort();
        Ok(presets)
    }

    /// Get list of presets from plugins
    fn get_plugin_presets(&self) -> Result<Vec<String>> {
        let discovery = match vm_plugin::PluginDiscovery::new() {
            Ok(d) => d,
            Err(_) => return Ok(Vec::new()), // No plugins available
        };

        let plugin_presets = discovery
            .get_all_presets()
            .into_iter()
            .map(|(_, preset)| preset.name.clone())
            .collect();

        Ok(plugin_presets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_preset_detection() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let presets_dir = crate::paths::get_presets_dir();

        // Create Django indicators
        fs::write(project_dir.join("manage.py"), "").unwrap();

        let detector = PresetDetector::new(project_dir, presets_dir);
        let preset = detector.detect();

        assert_eq!(preset, Some("django".to_string()));
    }

    #[test]
    fn test_react_detection() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let presets_dir = crate::paths::get_presets_dir();

        // Create package.json with React
        let package_json = r#"{
            "dependencies": {
                "react": "^18.0.0",
                "react-dom": "^18.0.0"
            }
        }"#;
        fs::write(project_dir.join("package.json"), package_json).unwrap();

        let detector = PresetDetector::new(project_dir, presets_dir);
        let preset = detector.detect();

        assert_eq!(preset, Some("react".to_string()));
    }
}
