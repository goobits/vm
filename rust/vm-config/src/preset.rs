use crate::config::VmConfig;
use anyhow::{Context, Result};
use glob::glob;
use serde::{Deserialize, Serialize};
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;
use vm_common::vm_error;
use crate::detector::detect_preset_for_project;

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
            let preset_file: PresetFile = serde_yaml::from_str(content)
                .with_context(|| format!("Failed to parse embedded preset '{}'", name))?;
            return Ok(preset_file.config);
        }

        // Fallback to file system (for custom presets)
        let preset_path = self.presets_dir.join(format!("{}.yaml", name));
        if !preset_path.exists() {
            vm_error!(
                "Preset '{}' not found (not embedded and no file at {:?})",
                name,
                preset_path
            );
            return Err(anyhow::anyhow!("Preset not found"));
        }

        let content = std::fs::read_to_string(&preset_path)?;
        let preset_file: PresetFile = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse preset '{}'", name))?;

        Ok(preset_file.config)
    }

    /// Get list of available presets
    pub fn list_presets(&self) -> Result<Vec<String>> {
        let mut presets = Vec::new();

        // Add embedded presets
        for name in crate::embedded_presets::get_preset_names() {
            presets.push(name.to_string());
        }

        // Add file system presets (if presets dir exists)
        if self.presets_dir.exists() {
            let pattern = self
                .presets_dir
                .join("*.yaml")
                .to_string_lossy()
                .to_string();
            for path in glob(&pattern)?.flatten() {
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
