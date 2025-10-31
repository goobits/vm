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
        // Try plugin presets first (user-facing presets)
        if let Some(config) = self.load_plugin_preset(name)? {
            return Ok(config);
        }

        // Try embedded presets second (system presets: base, tart-*)
        if let Some(content) = crate::embedded_presets::get_preset_content(name) {
            let source_desc = format!("embedded preset '{}'", name);
            let preset_file: PresetFile =
                crate::yaml::CoreOperations::parse_yaml_with_diagnostics(content, &source_desc)?;
            return Ok(preset_file.config);
        }

        // Fallback to file system (for custom presets)
        let preset_path = self.presets_dir.join(format!("{name}.yaml"));
        if !preset_path.exists() {
            vm_error!(
                "Preset '{}' not found (not embedded and no file at {:?})",
                name,
                preset_path
            );
            return Err(VmError::Config("Preset not found".to_string()));
        }

        let content = std::fs::read_to_string(&preset_path)?;
        let source_desc = format!("preset file '{}'", preset_path.display());
        let preset_file: PresetFile =
            crate::yaml::CoreOperations::parse_yaml_with_diagnostics(&content, &source_desc)?;

        Ok(preset_file.config)
    }

    /// Load a preset from plugins
    fn load_plugin_preset(&self, name: &str) -> Result<Option<VmConfig>> {
        let plugins = match vm_plugin::discover_plugins() {
            Ok(p) => p,
            Err(_) => return Ok(None), // No plugins available
        };

        // Find preset plugin by name
        let preset_plugin = plugins
            .iter()
            .find(|p| p.info.plugin_type == vm_plugin::PluginType::Preset && p.info.name == name);

        if let Some(plugin) = preset_plugin {
            // Load preset content
            let content = match vm_plugin::load_preset_content(plugin) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Warning: Failed to load preset content from plugin {name}: {e}");
                    return Ok(None);
                }
            };

            // Convert PresetContent to VmConfig
            let environment: indexmap::IndexMap<String, String> = content
                .environment
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            let aliases: indexmap::IndexMap<String, String> = content
                .aliases
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            // Convert services list to ServiceConfig map
            let mut services = indexmap::IndexMap::new();
            for service_name in &content.services {
                use crate::config::ServiceConfig;
                services.insert(
                    service_name.clone(),
                    ServiceConfig {
                        enabled: true,
                        ..Default::default()
                    },
                );
            }

            let config = VmConfig {
                apt_packages: content.packages,
                npm_packages: content.npm_packages,
                pip_packages: content.pip_packages,
                cargo_packages: content.cargo_packages,
                environment,
                aliases,
                services,
                ..Default::default()
            };

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
                .map_err(|e| VmError::Filesystem(format!("Glob pattern error: {e}")))?
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
        let plugins = match vm_plugin::discover_plugins() {
            Ok(p) => p,
            Err(_) => return Ok(Vec::new()), // No plugins available
        };

        let preset_names = vm_plugin::get_preset_plugins(&plugins)
            .into_iter()
            .map(|p| p.info.name.clone())
            .collect();

        Ok(preset_names)
    }

    /// Get description for a preset (from plugin or embedded)
    pub fn get_preset_description(&self, name: &str) -> Option<String> {
        // Try plugins first
        if let Ok(plugins) = vm_plugin::discover_plugins() {
            for plugin in plugins {
                if plugin.info.plugin_type == vm_plugin::PluginType::Preset
                    && plugin.info.name == name
                {
                    return plugin.info.description.clone();
                }
            }
        }

        // Fall back to embedded preset descriptions
        match name {
            "base" => Some("Generic development preset with basic tools".to_string()),
            "tart-linux" => Some("Ubuntu Linux VM for development on Apple Silicon".to_string()),
            "tart-macos" => Some("macOS VM for development on Apple Silicon".to_string()),
            "tart-ubuntu" => Some("Ubuntu Linux VM for Tart provider".to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_preset_detection() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let project_dir = temp_dir.path().to_path_buf();
        let presets_dir = crate::paths::get_presets_dir();

        // Create Django indicators
        fs::write(project_dir.join("manage.py"), "").expect("should write manage.py");

        let detector = PresetDetector::new(project_dir, presets_dir);
        let preset = detector.detect();

        assert_eq!(preset, Some("django".to_string()));
    }

    #[test]
    fn test_react_detection() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let project_dir = temp_dir.path().to_path_buf();
        let presets_dir = crate::paths::get_presets_dir();

        // Create package.json with React
        let package_json = r#"{
            "dependencies": {
                "react": "^18.0.0",
                "react-dom": "^18.0.0"
            }
        }"#;
        fs::write(project_dir.join("package.json"), package_json)
            .expect("should write package.json");

        let detector = PresetDetector::new(project_dir, presets_dir);
        let preset = detector.detect();

        assert_eq!(preset, Some("react".to_string()));
    }
}
