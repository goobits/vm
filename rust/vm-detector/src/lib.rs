//! VM project detection and analysis library.
//!
//! This library provides comprehensive project detection capabilities for various
//! programming languages, frameworks, and technologies. It analyzes project
//! directories to identify technologies in use and recommend appropriate VM
//! configurations.
//!
//! ## Main Features
//! - **Project Type Detection**: Automatically detect programming languages and frameworks
//! - **Preset Recommendations**: Suggest appropriate VM presets based on detected technologies
//! - **Multi-Technology Support**: Handle projects using multiple languages/frameworks
//! - **Tool Detection**: Identify installed development tools and runtimes
//! - **Host OS Detection**: Determine the host operating system and distribution
//!
//! ## Usage Examples
//!
//! ```rust
//! use std::path::Path;
//! use vm_detector::{detect_project_type, get_recommended_preset, detect_host_os};
//!
//! // Detect project technologies
//! let project_dir = Path::new("/path/to/project");
//! let detected_types = detect_project_type(project_dir);
//! println!("Detected: {:?}", detected_types);
//!
//! // Get recommended preset
//! let preset = get_recommended_preset(project_dir);
//! println!("Recommended preset: {}", preset);
//!
//! // Detect host OS
//! let os = detect_host_os();
//! println!("Host OS: {}", os);
//! ```

use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use vm_common::file_system::{has_any_dir, has_any_file, has_file, has_file_containing};

pub mod os;
pub mod presets;
pub mod tools;

pub use os::detect_host_os;
pub use presets::{
    detect_preset_for_project, get_detected_technologies, get_recommended_preset,
    is_multi_tech_project, is_react_project,
};
pub use tools::{detect_databases, detect_languages, has_command, ToolDetector};

/// Check if a directory contains a Python project.
///
/// Detects Python projects by looking for common Python project files:
/// - `requirements.txt` - pip dependencies
/// - `pyproject.toml` - modern Python project configuration
/// - `setup.py` - traditional Python package setup
/// - `Pipfile` - Pipenv dependency management
///
/// # Arguments
/// * `dir` - The directory path to check
///
/// # Returns
/// `true` if any Python project indicators are found, `false` otherwise
///
/// # Examples
/// ```rust
/// use std::path::Path;
/// use vm_detector::is_python_project;
///
/// let project_dir = Path::new("/path/to/python/project");
/// if is_python_project(project_dir) {
///     println!("This is a Python project");
/// }
/// ```
pub fn is_python_project(dir: &Path) -> bool {
    has_any_file(
        dir,
        &["requirements.txt", "pyproject.toml", "setup.py", "Pipfile"],
    )
}

/// Check if a directory is a pipx virtual environment.
///
/// Detects pipx environments by looking for the `pipx_metadata.json` file
/// that pipx creates in each isolated environment.
///
/// # Arguments
/// * `path` - The directory path to check
///
/// # Returns
/// `true` if the directory contains pipx metadata, `false` otherwise
///
/// # Examples
/// ```rust
/// use std::path::Path;
/// use vm_detector::is_pipx_environment;
///
/// let env_dir = Path::new("/home/user/.local/share/pipx/venvs/myapp");
/// if is_pipx_environment(env_dir) {
///     println!("This is a pipx environment");
/// }
/// ```
pub fn is_pipx_environment(path: &Path) -> bool {
    path.join("pipx_metadata.json").exists()
}

/// Detect all project types and technologies in a directory.
///
/// This is the core detection function that analyzes a project directory
/// to identify programming languages, frameworks, and tools in use.
/// It returns a set of detected technology identifiers.
///
/// ## Supported Technologies
/// - **JavaScript/Node.js**: `nodejs`, `react`, `vue`, `next`, `angular`
/// - **Python**: `python`, `django`, `flask`
/// - **Other Languages**: `rust`, `go`, `ruby`, `rails`, `php`
/// - **Infrastructure**: `docker`, `kubernetes`
///
/// ## Detection Logic
/// - Examines configuration files (package.json, Cargo.toml, etc.)
/// - Analyzes dependencies to identify frameworks
/// - Checks for infrastructure-related files
/// - Prioritizes more specific frameworks over generic languages
///
/// # Arguments
/// * `dir` - The project directory to analyze
///
/// # Returns
/// A `HashSet<String>` containing detected technology identifiers.
/// Returns empty set if no recognized technologies are found.
///
/// # Examples
/// ```rust
/// use std::path::Path;
/// use vm_detector::detect_project_type;
///
/// let project_dir = Path::new("/path/to/react/project");
/// let detected = detect_project_type(project_dir);
///
/// if detected.contains("react") {
///     println!("React project detected");
/// }
/// if detected.len() > 1 {
///     println!("Multi-technology project: {:?}", detected);
/// }
/// ```
pub fn detect_project_type(dir: &Path) -> HashSet<String> {
    let mut types = HashSet::new();

    // --- Node.js Detection ---
    if has_file(dir, "package.json") {
        if let Ok(content) = fs::read_to_string(dir.join("package.json")) {
            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                let mut framework = String::from("nodejs");
                let deps = json.get("dependencies").and_then(Value::as_object);
                let dev_deps = json.get("devDependencies").and_then(Value::as_object);

                let all_deps = deps.into_iter().chain(dev_deps).flat_map(|o| o.keys());

                for dep in all_deps {
                    match dep.as_str() {
                        "react" => {
                            framework = String::from("react");
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

/// Format detected project types for display output.
///
/// Converts the set of detected technologies into a human-readable string
/// suitable for CLI output or logging. Handles various cases including
/// single technology, multiple technologies, and fallback scenarios.
///
/// ## Output Formats
/// - Single technology: Returns the technology name (e.g., "react")
/// - Multiple technologies: Returns "multi:" prefix with space-separated list
/// - No technologies: Returns "generic" as fallback
/// - Results are sorted alphabetically for consistent output
///
/// # Arguments
/// * `detected_types` - Set of detected technology identifiers
///
/// # Returns
/// A formatted string representation of the detected technologies
///
/// # Examples
/// ```rust
/// use std::collections::HashSet;
/// use vm_detector::format_detected_types;
///
/// let mut types = HashSet::new();
/// types.insert("react".to_string());
/// types.insert("docker".to_string());
///
/// let formatted = format_detected_types(types);
/// assert_eq!(formatted, "multi:docker react");
/// ```
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
mod tests;
