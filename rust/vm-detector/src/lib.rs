use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use vm_common::file_system::*;

pub mod os;
pub mod tools;
pub mod presets;

pub use os::*;
pub use tools::*;
pub use presets::*;

/// Check if a path is a Python project (has setup.py, pyproject.toml, etc.)
pub fn is_python_project(dir: &Path) -> bool {
    has_any_file(
        dir,
        &["requirements.txt", "pyproject.toml", "setup.py", "Pipfile"],
    )
}

/// Check if a path is a pipx environment
pub fn is_pipx_environment(path: &Path) -> bool {
    path.join("pipx_metadata.json").exists()
}

/// Detect project type(s) in a given directory
pub fn detect_project_type(dir: &Path) -> HashSet<String> {
    let mut types = HashSet::new();

    // --- Node.js Detection ---
    if has_file(dir, "package.json") {
        if let Ok(content) = fs::read_to_string(dir.join("package.json")) {
            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                let mut framework = "nodejs".to_string();
                let deps = json.get("dependencies").and_then(Value::as_object);
                let dev_deps = json.get("devDependencies").and_then(Value::as_object);

                let all_deps = deps.into_iter().chain(dev_deps).flat_map(|o| o.keys());

                for dep in all_deps {
                    match dep.as_str() {
                        "react" => {
                            framework = "react".to_string();
                            break;
                        }
                        "vue" => {
                            framework = "vue".to_string();
                            break;
                        }
                        "next" => {
                            framework = "next".to_string();
                            break;
                        }
                        "@angular/core" => {
                            framework = "angular".to_string();
                            break;
                        }
                        _ => (),
                    }
                }
                types.insert(framework);
            }
            // If JSON parsing fails, we don't add nodejs type (graceful degradation)
        }
    }

    // --- Python Detection ---
    if is_python_project(dir) {
        let mut framework = "python".to_string();
        if has_file_containing(dir, "requirements.txt", "Django")
            || has_file_containing(dir, "requirements.txt", "django")
        {
            framework = "django".to_string();
        } else if has_file_containing(dir, "requirements.txt", "Flask")
            || has_file_containing(dir, "requirements.txt", "flask")
        {
            framework = "flask".to_string();
        }
        types.insert(framework);
    }

    // --- Rust Detection ---
    if has_file(dir, "Cargo.toml") {
        types.insert("rust".to_string());
    }

    // --- Go Detection ---
    if has_file(dir, "go.mod") {
        types.insert("go".to_string());
    }

    // --- Ruby Detection ---
    if has_file(dir, "Gemfile") {
        let mut framework = "ruby".to_string();
        if has_file_containing(dir, "Gemfile", "rails") {
            framework = "rails".to_string();
        }
        types.insert(framework);
    }

    // --- PHP Detection ---
    if has_file(dir, "composer.json") {
        types.insert("php".to_string());
    }

    // --- Docker Detection ---
    if has_any_file(
        dir,
        &["Dockerfile", "docker-compose.yml", "docker-compose.yaml"],
    ) {
        types.insert("docker".to_string());
    }

    // --- Kubernetes Detection ---
    if has_any_file(dir, &["k8s.yaml", "k8s.yml"]) || has_any_dir(dir, &["kubernetes", "k8s"]) {
        types.insert("kubernetes".to_string());
    }

    types
}

/// Format detected types for output (used by binary)
pub fn format_detected_types(detected_types: HashSet<String>) -> String {
    let mut sorted_types: Vec<String> = detected_types.into_iter().collect();
    sorted_types.sort(); // Sort for deterministic output

    if sorted_types.is_empty() {
        "generic".to_string()
    } else if sorted_types.len() == 1 {
        sorted_types[0].clone()
    } else {
        format!("multi:{}", sorted_types.join(" "))
    }
}

// --- Helper Functions ---
// (Now using shared utilities from vm_common::file_system)

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Test fixture for creating temporary project directories
    struct ProjectTestFixture {
        _temp_dir: TempDir,
        project_dir: std::path::PathBuf,
    }

    impl ProjectTestFixture {
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
    fn test_react_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "package.json",
                r#"
        {
          "name": "test-react-app",
          "version": "1.0.0",
          "dependencies": {
            "react": "^18.2.0",
            "react-dom": "^18.2.0"
          }
        }
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("react"));
        assert!(!detected.contains("nodejs"));

        let formatted = format_detected_types(detected);
        assert_eq!(formatted, "react");
    }

    #[test]
    fn test_vue_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "package.json",
                r#"
        {
          "name": "test-vue-app",
          "dependencies": {
            "vue": "^3.3.0"
          }
        }
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("vue"));

        let formatted = format_detected_types(detected);
        assert_eq!(formatted, "vue");
    }

    #[test]
    fn test_next_detection_overrides_react() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "package.json",
                r#"
        {
          "name": "test-nextjs-app",
          "dependencies": {
            "next": "^13.4.0",
            "react": "^18.2.0",
            "react-dom": "^18.2.0"
          }
        }
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("next"));
        assert!(!detected.contains("react"));
    }

    #[test]
    fn test_angular_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "package.json",
                r#"
        {
          "name": "test-angular-app",
          "dependencies": {
            "@angular/core": "^15.2.0",
            "@angular/common": "^15.2.0"
          }
        }
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("angular"));
    }

    #[test]
    fn test_django_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file("requirements.txt", "Django==4.2.0\npsycopg2-binary==2.9.6")
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("django"));
        assert!(!detected.contains("python"));
    }

    #[test]
    fn test_flask_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file("requirements.txt", "Flask==2.3.0\nFlask-SQLAlchemy==3.0.5")
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("flask"));
    }

    #[test]
    fn test_rails_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "Gemfile",
                r#"
        source 'https://rubygems.org'
        gem 'rails', '~> 7.0.0'
        gem 'pg', '~> 1.1'
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("rails"));
        assert!(!detected.contains("ruby"));
    }

    #[test]
    fn test_nodejs_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "package.json",
                r#"
        {
          "name": "test-nodejs-app",
          "dependencies": {
            "express": "^4.18.0",
            "lodash": "^4.17.21"
          }
        }
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("nodejs"));
    }

    #[test]
    fn test_python_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file("requirements.txt", "requests==2.31.0\nnumpy==1.24.0")
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("python"));
    }

    #[test]
    fn test_rust_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "Cargo.toml",
                r#"
        [package]
        name = "test-rust-app"
        version = "0.1.0"
        edition = "2021"

        [dependencies]
        tokio = "1.0"
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("rust"));
    }

    #[test]
    fn test_go_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "go.mod",
                r#"
        module test-go-app

        go 1.20

        require (
            github.com/gin-gonic/gin v1.9.1
        )
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("go"));
    }

    #[test]
    fn test_docker_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file("Dockerfile", "FROM node:18-alpine\nWORKDIR /app")
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("docker"));
    }

    #[test]
    fn test_docker_compose_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "docker-compose.yml",
                r#"
        version: '3.8'
        services:
          app:
            build: .
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("docker"));
    }

    #[test]
    fn test_kubernetes_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture.create_dir("k8s").unwrap();
        fixture
            .create_file(
                "k8s/deployment.yaml",
                r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test-app
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("kubernetes"));
    }

    #[test]
    fn test_multi_tech_detection() {
        let fixture = ProjectTestFixture::new().unwrap();

        // Create both React and Django indicators
        fixture
            .create_file(
                "package.json",
                r#"
        {
          "name": "fullstack-app",
          "dependencies": {
            "react": "^18.2.0"
          }
        }
        "#,
            )
            .unwrap();

        fixture
            .create_file("requirements.txt", "Django==4.2.0")
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("react"));
        assert!(detected.contains("django"));

        let formatted = format_detected_types(detected);
        assert!(formatted.starts_with("multi:"));
        assert!(formatted.contains("django"));
        assert!(formatted.contains("react"));
    }

    #[test]
    fn test_multi_tech_with_docker() {
        let fixture = ProjectTestFixture::new().unwrap();

        fixture
            .create_file(
                "package.json",
                r#"
        {
          "name": "dockerized-react-app",
          "dependencies": {
            "react": "^18.2.0"
          }
        }
        "#,
            )
            .unwrap();

        fixture
            .create_file("Dockerfile", "FROM node:18-alpine")
            .unwrap();

        let detected = detect_project_type(fixture.path());
        let formatted = format_detected_types(detected);

        assert!(formatted.starts_with("multi:"));
        assert!(formatted.contains("docker"));
        assert!(formatted.contains("react"));
    }

    #[test]
    fn test_empty_directory() {
        let fixture = ProjectTestFixture::new().unwrap();

        let detected = detect_project_type(fixture.path());
        let formatted = format_detected_types(detected);

        assert_eq!(formatted, "generic");
    }

    #[test]
    fn test_malformed_json_graceful_handling() {
        let fixture = ProjectTestFixture::new().unwrap();

        // Create malformed JSON
        fixture
            .create_file(
                "package.json",
                r#"
        {
          "name": "broken-app",
          "version": "1.0.0"
          "dependencies": {
            "react": "^18.2.0"
          // Missing closing braces
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        let formatted = format_detected_types(detected);

        // Should gracefully fall back to generic when JSON is malformed
        assert_eq!(formatted, "generic");
    }

    #[test]
    fn test_missing_files_handling() {
        let _fixture = ProjectTestFixture::new().unwrap();

        // Test with various non-existent files
        let detected = detect_project_type(Path::new("/nonexistent/path"));
        let formatted = format_detected_types(detected);

        assert_eq!(formatted, "generic");
    }

    #[test]
    fn test_php_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "composer.json",
                r#"
        {
          "name": "test/php-app",
          "require": {
            "php": ">=8.0"
          }
        }
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("php"));
    }

    #[test]
    fn test_pyproject_toml_detection() {
        let fixture = ProjectTestFixture::new().unwrap();
        fixture
            .create_file(
                "pyproject.toml",
                r#"
        [tool.poetry]
        name = "test-python-app"
        version = "0.1.0"

        [tool.poetry.dependencies]
        python = "^3.9"
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());
        assert!(detected.contains("python"));
    }

    #[test]
    fn test_framework_priority_ordering() {
        // Test that specific frameworks take priority over generic ones
        let fixture = ProjectTestFixture::new().unwrap();

        // Create package.json with Next.js (should override React detection)
        fixture
            .create_file(
                "package.json",
                r#"
        {
          "name": "test-app",
          "dependencies": {
            "react": "^18.2.0",
            "react-dom": "^18.2.0",
            "next": "^13.4.0"
          }
        }
        "#,
            )
            .unwrap();

        let detected = detect_project_type(fixture.path());

        // Should detect Next.js, not React (Next.js wins due to break statement)
        assert!(detected.contains("next"));
        assert!(!detected.contains("react"));
        assert!(!detected.contains("nodejs"));
    }
}
