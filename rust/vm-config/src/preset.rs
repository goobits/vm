use crate::config::VmConfig;
use anyhow::{Result, Context};
use std::path::PathBuf;
use glob::glob;
use which::which;

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
    pub fn detect(&self) -> Result<Option<String>> {
        // Check for various project indicators
        let detections = vec![
            // Django
            (vec!["manage.py", "django", "settings.py"], "django"),
            // Rails
            (vec!["Gemfile", "config.ru", "app/controllers"], "rails"),
            // React
            (vec!["package.json"], "react"), // Further check needed
            // Node.js
            (vec!["package.json", "node_modules"], "nodejs"),
            // Python
            (vec!["requirements.txt", "setup.py", "pyproject.toml"], "python"),
            // Rust
            (vec!["Cargo.toml", "Cargo.lock"], "rust"),
            // Go
            (vec!["go.mod", "go.sum"], "go"),
            // Docker Compose
            (vec!["docker-compose.yml", "docker-compose.yaml"], "docker"),
            // Kubernetes
            (vec!["k8s/", "kubernetes/", "*.yaml"], "kubernetes"),
        ];

        for (indicators, preset) in detections {
            if self.has_indicators(&indicators) {
                // Special handling for React vs Node.js
                if preset == "react" && self.is_react_project()? {
                    return Ok(Some("react".to_string()));
                } else if preset == "react" {
                    continue; // Not React, check other presets
                }
                return Ok(Some(preset.to_string()));
            }
        }

        Ok(None)
    }

    /// Check if project has any of the given indicator files/directories
    fn has_indicators(&self, indicators: &[&str]) -> bool {
        for indicator in indicators {
            let path = self.project_dir.join(indicator);

            // Check if it's a glob pattern
            if indicator.contains('*') {
                let pattern = self.project_dir.join(indicator).to_string_lossy().to_string();
                if let Ok(paths) = glob(&pattern) {
                    if paths.count() > 0 {
                        return true;
                    }
                }
            } else if path.exists() {
                return true;
            }
        }
        false
    }

    /// Check if a package.json indicates a React project
    fn is_react_project(&self) -> Result<bool> {
        let package_json = self.project_dir.join("package.json");
        if !package_json.exists() {
            return Ok(false);
        }

        let content = std::fs::read_to_string(&package_json)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;

        // Check dependencies for React
        if let Some(deps) = json.get("dependencies") {
            if deps.get("react").is_some() || deps.get("react-dom").is_some() {
                return Ok(true);
            }
        }

        if let Some(deps) = json.get("devDependencies") {
            if deps.get("react").is_some() || deps.get("react-scripts").is_some() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Load a preset configuration by name
    pub fn load_preset(&self, name: &str) -> Result<VmConfig> {
        let preset_file = self.presets_dir.join(format!("{}.yaml", name));

        if !preset_file.exists() {
            anyhow::bail!("Preset '{}' not found at {:?}", name, preset_file);
        }

        VmConfig::from_file(&preset_file)
            .with_context(|| format!("Failed to load preset '{}'", name))
    }

    /// Get list of available presets
    pub fn list_presets(&self) -> Result<Vec<String>> {
        let pattern = self.presets_dir.join("*.yaml").to_string_lossy().to_string();
        let mut presets = Vec::new();

        for entry in glob(&pattern)? {
            if let Ok(path) = entry {
                if let Some(stem) = path.file_stem() {
                    presets.push(stem.to_string_lossy().to_string());
                }
            }
        }

        presets.sort();
        Ok(presets)
    }
}

/// Detect installed tools/languages for smart preset selection
#[allow(dead_code)]
pub struct ToolDetector;

#[allow(dead_code)]
impl ToolDetector {
    /// Check if a command is available in PATH
    pub fn has_command(cmd: &str) -> bool {
        which(cmd).is_ok()
    }

    /// Detect installed language runtimes
    pub fn detect_languages() -> Vec<String> {
        let mut languages = Vec::new();

        if Self::has_command("node") || Self::has_command("npm") {
            languages.push("nodejs".to_string());
        }
        if Self::has_command("python") || Self::has_command("python3") {
            languages.push("python".to_string());
        }
        if Self::has_command("ruby") || Self::has_command("gem") {
            languages.push("ruby".to_string());
        }
        if Self::has_command("cargo") || Self::has_command("rustc") {
            languages.push("rust".to_string());
        }
        if Self::has_command("go") {
            languages.push("go".to_string());
        }
        if Self::has_command("java") || Self::has_command("javac") {
            languages.push("java".to_string());
        }

        languages
    }

    /// Detect installed databases
    pub fn detect_databases() -> Vec<String> {
        let mut databases = Vec::new();

        if Self::has_command("psql") {
            databases.push("postgresql".to_string());
        }
        if Self::has_command("mysql") {
            databases.push("mysql".to_string());
        }
        if Self::has_command("mongosh") || Self::has_command("mongo") {
            databases.push("mongodb".to_string());
        }
        if Self::has_command("redis-cli") {
            databases.push("redis".to_string());
        }

        databases
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
        let presets_dir = PathBuf::from("/workspace/configs/presets");

        // Create Django indicators
        fs::write(project_dir.join("manage.py"), "").unwrap();

        let detector = PresetDetector::new(project_dir.clone(), presets_dir);
        let preset = detector.detect().unwrap();

        assert_eq!(preset, Some("django".to_string()));
    }

    #[test]
    fn test_react_detection() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().to_path_buf();
        let presets_dir = PathBuf::from("/workspace/configs/presets");

        // Create package.json with React
        let package_json = r#"{
            "dependencies": {
                "react": "^18.0.0",
                "react-dom": "^18.0.0"
            }
        }"#;
        fs::write(project_dir.join("package.json"), package_json).unwrap();

        let detector = PresetDetector::new(project_dir.clone(), presets_dir);
        let preset = detector.detect().unwrap();

        assert_eq!(preset, Some("react".to_string()));
    }
}