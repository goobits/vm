use crate::config::{ImageSpec, VmConfig, VmSettings};
use crate::detector::detect_preset_for_project;
use serde::{Deserialize, Serialize};
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;
use tracing::instrument;
use vm_core::error::{Result, VmError};
use vm_core::vm_warning;

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
///
/// Manages preset discovery from canonical sources:
/// - Plugin presets from `~/.vm/plugins/presets/`
/// - Embedded presets (base, tart-*, etc.)
pub struct PresetDetector {
    project_dir: PathBuf,
}

impl PresetDetector {
    /// Creates a new preset detector
    ///
    /// # Arguments
    ///
    /// * `project_dir` - Directory to scan for project-specific detection
    pub fn new(project_dir: PathBuf) -> Self {
        Self { project_dir }
    }

    /// Detects the appropriate preset based on project files
    ///
    /// Scans the project directory for technology indicators (package.json, Cargo.toml, etc.)
    /// and returns the name of a matching preset.
    ///
    /// # Returns
    ///
    /// Some(preset_name) if a matching preset is found, None otherwise
    pub fn detect(&self) -> Option<String> {
        // Use vm-detector's comprehensive detection logic
        detect_preset_for_project(&self.project_dir)
    }

    /// Loads a preset configuration by name
    ///
    /// Searches for presets in order:
    /// 1. Plugin presets (user-installed)
    /// 2. Embedded presets (system defaults)
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the preset to load
    ///
    /// # Returns
    ///
    /// VmConfig with the preset's configuration
    ///
    /// # Errors
    ///
    /// Returns error if preset is not found or cannot be parsed
    #[instrument(skip(self), fields(name = %name))]
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

        Err(VmError::Config(format!(
            "Preset '{name}' not found (no installed plugin or embedded preset)"
        )))
    }

    /// Load a preset from plugins
    #[instrument(skip(self), fields(name = %name))]
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
            let mut content = match vm_plugin::load_preset_content(plugin) {
                Ok(c) => c,
                Err(e) => {
                    vm_warning!("Failed to load preset content from plugin {name}: {e}");
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

            let vm = content.vm_image.take().map(|value| VmSettings {
                image: Some(ImageSpec::String(value)),
                ..Default::default()
            });

            // Deserialize networking from YAML value if present
            let networking = content
                .networking
                .as_ref()
                .and_then(|v| serde_yaml_ng::from_value(v.clone()).ok());

            // Deserialize host_sync from YAML value if present
            let host_sync = content
                .host_sync
                .as_ref()
                .and_then(|v| serde_yaml_ng::from_value(v.clone()).ok());

            // Deserialize terminal from YAML value if present
            let terminal = content
                .terminal
                .as_ref()
                .and_then(|v| serde_yaml_ng::from_value(v.clone()).ok());

            let mounts = content
                .mounts
                .as_ref()
                .and_then(|value| serde_yaml_ng::from_value(value.clone()).ok())
                .unwrap_or_default();

            let tools = content
                .tools
                .as_ref()
                .and_then(|value| serde_yaml_ng::from_value(value.clone()).ok())
                .unwrap_or_default();

            let config = VmConfig {
                apt_packages: content.packages,
                npm_packages: content.npm_packages,
                pip_packages: content.pip_packages,
                cargo_packages: content.cargo_packages,
                environment,
                aliases,
                services,
                vm,
                networking,
                host_sync,
                terminal,
                mounts,
                tools,
                ..Default::default()
            };

            return Ok(Some(config));
        }

        Ok(None)
    }

    /// Lists available provision presets (excludes image presets)
    ///
    /// This is used by `vm config preset` to show presets that can be merged
    /// into existing configurations. Image presets are excluded because they
    /// are only used during project initialization.
    ///
    /// # Returns
    ///
    /// Sorted vector of provision preset names
    #[instrument(skip(self))]
    pub fn list_presets(&self) -> Result<Vec<String>> {
        let mut presets = Vec::new();

        // Add embedded presets
        for name in crate::embedded_presets::get_preset_names() {
            presets.push(name.to_string());
        }

        // Add plugin presets (filter out image presets)
        if let Ok(plugin_presets) = self.get_plugin_presets() {
            for name in plugin_presets {
                if !presets.contains(&name) {
                    presets.push(name);
                }
            }
        }

        presets.sort();
        Ok(presets)
    }

    /// Get list of presets from plugins (provision presets only, excludes image presets)
    pub(crate) fn get_plugin_presets(&self) -> Result<Vec<String>> {
        let plugins = match vm_plugin::discover_plugins() {
            Ok(p) => p,
            Err(_) => return Ok(Vec::new()), // No plugins available
        };

        let preset_names = vm_plugin::get_preset_plugins(&plugins)
            .into_iter()
            .filter(|p| {
                // Filter out image presets - only include provision presets
                if let Some(category) = &p.info.preset_category {
                    category != &vm_plugin::PresetCategory::Image
                } else {
                    // If category not specified, check preset content
                    if let Ok(content) = vm_plugin::load_preset_content(p) {
                        content.category != vm_plugin::PresetCategory::Image
                    } else {
                        true // Include if we can't determine category
                    }
                }
            })
            .map(|p| p.info.name.clone())
            .collect();

        Ok(preset_names)
    }

    /// Lists all available presets including both image and provision types.
    ///
    /// This is used by project initialization to validate preset names.
    /// For filtering to provision-only presets, use [`list_presets`](Self::list_presets).
    ///
    /// # Returns
    ///
    /// Sorted vector of preset names from installed plugins and embedded presets.
    #[instrument(skip(self))]
    pub fn list_all_presets(&self) -> Result<Vec<String>> {
        let mut presets = Vec::new();

        // Add embedded presets
        for name in crate::embedded_presets::get_preset_names() {
            presets.push(name.to_string());
        }

        // Add ALL plugin presets (no filtering)
        if let Ok(plugins) = vm_plugin::discover_plugins() {
            for plugin in vm_plugin::get_preset_plugins(&plugins) {
                let name = plugin.info.name.clone();
                if !presets.contains(&name) {
                    presets.push(name);
                }
            }
        }

        presets.sort();
        Ok(presets)
    }

    /// Gets description for a preset (from plugin or embedded)
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the preset
    ///
    /// # Returns
    ///
    /// Some(description) if found, None otherwise
    #[instrument(skip(self), fields(name = %name))]
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
            "vibe-tart" => {
                Some("Vibe Development on Tart with Docker-capable Linux guests".to_string())
            }
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

        // Create Django indicators
        fs::write(project_dir.join("manage.py"), "").expect("should write manage.py");

        let detector = PresetDetector::new(project_dir);
        let preset = detector.detect();

        assert_eq!(preset, Some("django".to_string()));
    }

    #[test]
    fn test_react_detection() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let project_dir = temp_dir.path().to_path_buf();

        // Create package.json with React
        let package_json = r#"{
            "dependencies": {
                "react": "^18.0.0",
                "react-dom": "^18.0.0"
            }
        }"#;
        fs::write(project_dir.join("package.json"), package_json)
            .expect("should write package.json");

        let detector = PresetDetector::new(project_dir);
        let preset = detector.detect();

        assert_eq!(preset, Some("react".to_string()));
    }
}
