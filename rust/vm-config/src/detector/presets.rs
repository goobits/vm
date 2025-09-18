use super::detect_project_type;
use glob::glob;
use std::collections::HashSet;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

/// Detect the most appropriate VM preset for a project directory.
///
/// This function analyzes a project directory using comprehensive detection logic
/// and returns the highest-priority preset that matches the detected technologies.
/// Presets are VM configuration templates optimized for specific tech stacks.
///
/// ## Preset Priority Order
/// 1. **Framework-specific**: next, react, angular, vue, django, flask, rails
/// 2. **Language-specific**: nodejs, python, rust, go, php
/// 3. **Infrastructure**: docker, kubernetes
///
/// ## Supported Presets
/// - `next` - Next.js applications
/// - `react` - React applications
/// - `angular` - Angular applications
/// - `vue` - Vue.js applications
/// - `django` - Django web applications
/// - `flask` - Flask web applications
/// - `rails` - Ruby on Rails applications
/// - `nodejs` - Node.js applications
/// - `python` - Python projects
/// - `rust` - Rust projects
/// - `go` - Go projects
/// - `php` - PHP projects
/// - `docker` - Dockerized applications
/// - `kubernetes` - Kubernetes deployments
///
/// # Arguments
/// * `project_dir` - The project directory to analyze
///
/// # Returns
/// * `Some(preset_name)` - The recommended preset name
/// * `None` - If no matching preset is found
///
/// # Examples
/// ```rust
/// use std::path::Path;
/// use vm_detector::detect_preset_for_project;
///
/// let project_dir = Path::new("/path/to/react/app");
/// if let Some(preset) = detect_preset_for_project(project_dir) {
///     println!("Recommended preset: {}", preset);
/// }
/// ```
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
    if has_any_dir(
        project_dir,
        &["k8s", "kubernetes", "helm", "charts", ".k8s"],
    ) {
        return Some("kubernetes".to_string());
    }

    // Additional file pattern checks
    let k8s_patterns = [
        "**/kustomization.yaml",
        "**/deployment.yaml",
        "**/service.yaml",
    ];
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

/// Check if a directory contains a React project.
///
/// Detects React projects by analyzing the project structure and dependencies.
/// This includes both standard React applications and Next.js projects which
/// are built on top of React.
///
/// ## Detection Logic
/// - Checks for React dependencies in package.json
/// - Identifies Next.js projects (which are React-based)
/// - Uses vm-detector's comprehensive project analysis
///
/// # Arguments
/// * `project_dir` - The directory path to check
///
/// # Returns
/// `true` if the project uses React or Next.js, `false` otherwise
///
/// # Examples
/// ```rust
/// use std::path::Path;
/// use vm_detector::is_react_project;
///
/// let project_dir = Path::new("/path/to/my-app");
/// if is_react_project(project_dir) {
///     println!("This project uses React");
/// }
/// ```
pub fn is_react_project(project_dir: &Path) -> bool {
    let detected_types = detect_project_type(project_dir);
    detected_types.contains("react") || detected_types.contains("next")
}

/// Get the recommended VM preset for a project with fallback.
///
/// This is a convenience function that wraps `detect_preset_for_project`
/// and provides a sensible fallback. If no specific preset is detected,
/// it returns "base" which provides a minimal VM configuration.
///
/// ## Fallback Strategy
/// - If a specific preset is detected → returns that preset name
/// - If no preset is detected → returns "base" as a safe default
///
/// The "base" preset typically includes:
/// - Basic development tools
/// - Common utilities
/// - Minimal resource allocation
///
/// # Arguments
/// * `project_dir` - The project directory to analyze
///
/// # Returns
/// The recommended preset name (never returns `None`)
///
/// # Examples
/// ```rust
/// use std::path::Path;
/// use vm_detector::get_recommended_preset;
///
/// let project_dir = Path::new("/path/to/project");
/// let preset = get_recommended_preset(project_dir);
/// println!("Using preset: {}", preset); // Always returns a value
/// ```
pub fn get_recommended_preset(project_dir: &Path) -> String {
    detect_preset_for_project(project_dir).unwrap_or_else(|| "base".to_string())
}

/// Check if a project uses multiple technologies.
///
/// Determines whether a project is a multi-technology stack by counting
/// the number of distinct technologies detected. This is useful for:
/// - Identifying complex project structures
/// - Determining if special handling is needed
/// - Providing appropriate warnings or recommendations
///
/// ## Multi-Tech Examples
/// - React frontend + Docker containerization
/// - Python backend + Node.js build tools
/// - Ruby on Rails + Kubernetes deployment
///
/// # Arguments
/// * `project_dir` - The project directory to analyze
///
/// # Returns
/// `true` if more than one technology is detected, `false` otherwise
///
/// # Examples
/// ```rust
/// use std::path::Path;
/// use vm_detector::is_multi_tech_project;
///
/// let project_dir = Path::new("/path/to/fullstack/app");
/// if is_multi_tech_project(project_dir) {
///     println!("Complex multi-technology project detected");
/// }
/// ```
pub fn is_multi_tech_project(project_dir: &Path) -> bool {
    let detected_types = detect_project_type(project_dir);
    detected_types.len() > 1
}

/// Get the complete set of detected technologies for a project.
///
/// Returns all detected technology identifiers for detailed analysis.
/// This provides the raw detection results, useful for:
/// - Detailed project analysis
/// - Custom preset logic
/// - Technology reporting and metrics
/// - Multi-technology handling
///
/// ## Technology Categories
/// - **Languages**: python, nodejs, rust, go, ruby, php
/// - **Frameworks**: react, vue, angular, next, django, flask, rails
/// - **Infrastructure**: docker, kubernetes
///
/// # Arguments
/// * `project_dir` - The project directory to analyze
///
/// # Returns
/// A `HashSet<String>` containing all detected technology identifiers
///
/// # Examples
/// ```rust
/// use std::path::Path;
/// use vm_detector::get_detected_technologies;
///
/// let project_dir = Path::new("/path/to/project");
/// let technologies = get_detected_technologies(project_dir);
///
/// for tech in &technologies {
///     println!("Detected: {}", tech);
/// }
///
/// if technologies.contains("docker") && technologies.contains("react") {
///     println!("Containerized React application");
/// }
/// ```
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
            .create_file("package.json", r#"{"dependencies": {"react": "^18.0.0"}}"#)
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
            .create_file("package.json", r#"{"dependencies": {"react": "^18.0.0"}}"#)
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
