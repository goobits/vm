use crate::detect_project_type;
use glob::glob;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Simplified preset detection that leverages vm-detector's core project detection
pub fn detect_preset_for_project(project_dir: &Path) -> Option<String> {
    // Use vm-detector's comprehensive project detection
    let detected_types = detect_project_type(project_dir);

    // Convert vm-detector results to preset names with priority
    let priority_presets = [
        ("next", "next"),
        ("react", "react"),
        ("angular", "angular"),
        ("vue", "vue"),
        ("django", "django"),
        ("flask", "flask"),
        ("rails", "rails"),
        ("nodejs", "nodejs"),
        ("python", "python"),
        ("rust", "rust"),
        ("go", "go"),
        ("php", "php"),
        ("docker", "docker"),
        ("kubernetes", "kubernetes"),
    ];

    // Return the highest priority preset found
    for (detected_type, preset_name) in &priority_presets {
        if detected_types.contains(*detected_type) {
            return Some(preset_name.to_string());
        }
    }

    // Fallback for additional project structure checks
    detect_preset_by_structure(project_dir)
}

/// Additional structure-based detection for edge cases
fn detect_preset_by_structure(project_dir: &Path) -> Option<String> {
    // Django project structure detection
    if has_file(project_dir, "manage.py") || has_dir(project_dir, "django") {
        return Some("django".to_string());
    }

    // Rails project structure detection
    if has_file(project_dir, "config.ru") || has_dir(project_dir, "app/controllers") {
        return Some("rails".to_string());
    }

    // Kubernetes structure detection
    if has_any_dir(project_dir, &["k8s", "kubernetes", "helm", "charts", ".k8s"]) {
        return Some("kubernetes".to_string());
    }

    // Additional file pattern checks
    let k8s_patterns = ["**/kustomization.yaml", "**/deployment.yaml", "**/service.yaml"];
    for pattern in &k8s_patterns {
        let full_pattern = project_dir.join(pattern).to_string_lossy().to_string();
        if let Ok(paths) = glob(&full_pattern) {
            if paths.count() > 0 {
                return Some("kubernetes".to_string());
            }
        }
    }

    None
}

/// Check if project has a specific file
fn has_file(base_dir: &Path, file_name: &str) -> bool {
    base_dir.join(file_name).exists()
}

/// Check if project has a specific directory
fn has_dir(base_dir: &Path, dir_name: &str) -> bool {
    base_dir.join(dir_name).is_dir()
}

/// Check if project has any of the given directories
fn has_any_dir(base_dir: &Path, dir_names: &[&str]) -> bool {
    dir_names.iter().any(|d| has_dir(base_dir, d))
}

/// Enhanced React project detection using vm-detector's logic
pub fn is_react_project(project_dir: &Path) -> bool {
    let detected_types = detect_project_type(project_dir);
    detected_types.contains("react") || detected_types.contains("next")
}

/// Get recommended preset based on detected project types
pub fn get_recommended_preset(project_dir: &Path) -> String {
    detect_preset_for_project(project_dir).unwrap_or_else(|| "base".to_string())
}

/// Check if detected types indicate a multi-technology project
pub fn is_multi_tech_project(project_dir: &Path) -> bool {
    let detected_types = detect_project_type(project_dir);
    detected_types.len() > 1
}

/// Get all detected technology types for a project
pub fn get_detected_technologies(project_dir: &Path) -> HashSet<String> {
    detect_project_type(project_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    struct PresetTestFixture {
        _temp_dir: TempDir,
        project_dir: PathBuf,
    }

    impl PresetTestFixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let temp_dir = TempDir::new()?;
            let project_dir = temp_dir.path().to_path_buf();

            Ok(Self {
                _temp_dir: temp_dir,
                project_dir,
            })
        }

        fn create_file(&self, name: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
            fs::write(self.project_dir.join(name), content)?;
            Ok(())
        }

        fn create_dir(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
            fs::create_dir_all(self.project_dir.join(name))?;
            Ok(())
        }

        fn path(&self) -> &Path {
            &self.project_dir
        }
    }

    #[test]
    fn test_react_preset_detection() {
        let fixture = PresetTestFixture::new().unwrap();
        fixture
            .create_file(
                "package.json",
                r#"{"dependencies": {"react": "^18.0.0"}}"#,
            )
            .unwrap();

        let preset = detect_preset_for_project(fixture.path());
        assert_eq!(preset, Some("react".to_string()));
    }

    #[test]
    fn test_django_preset_detection() {
        let fixture = PresetTestFixture::new().unwrap();
        fixture.create_file("manage.py", "").unwrap();

        let preset = detect_preset_for_project(fixture.path());
        assert_eq!(preset, Some("django".to_string()));
    }

    #[test]
    fn test_kubernetes_preset_detection() {
        let fixture = PresetTestFixture::new().unwrap();
        fixture.create_dir("k8s").unwrap();

        let preset = detect_preset_for_project(fixture.path());
        assert_eq!(preset, Some("kubernetes".to_string()));
    }

    #[test]
    fn test_multi_tech_detection() {
        let fixture = PresetTestFixture::new().unwrap();
        fixture
            .create_file(
                "package.json",
                r#"{"dependencies": {"react": "^18.0.0"}}"#,
            )
            .unwrap();
        fixture.create_file("Dockerfile", "FROM node:18").unwrap();

        assert!(is_multi_tech_project(fixture.path()));

        let technologies = get_detected_technologies(fixture.path());
        assert!(technologies.contains("react"));
        assert!(technologies.contains("docker"));
    }

    #[test]
    fn test_recommended_preset_fallback() {
        let fixture = PresetTestFixture::new().unwrap();
        // Empty directory should fallback to "base"

        let preset = get_recommended_preset(fixture.path());
        assert_eq!(preset, "base");
    }

    #[test]
    fn test_next_priority_over_react() {
        let fixture = PresetTestFixture::new().unwrap();
        fixture
            .create_file(
                "package.json",
                r#"{"dependencies": {"next": "^13.0.0", "react": "^18.0.0"}}"#,
            )
            .unwrap();

        let preset = detect_preset_for_project(fixture.path());
        assert_eq!(preset, Some("next".to_string()));
    }
}