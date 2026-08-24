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
//!
//! ## Usage Examples
//!
//! ```rust
//! use std::path::Path;
//! use vm_config::detector::{detect_project_type, get_recommended_preset};
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
//! ```

use serde_json::Value;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;
use tracing::warn;
use vm_core::error::{Result, VmError};
use vm_core::file_system::has_file_containing;

pub mod git;
pub mod presets;
mod project;
pub mod tools;

pub use presets::{
    detect_preset_for_project, get_detected_technologies, get_recommended_preset,
    is_multi_tech_project, is_react_project,
};
pub use project::ProjectFacts;
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
/// use vm_config::detector::is_python_project;
///
/// let project_dir = Path::new("/path/to/python/project");
/// if is_python_project(project_dir) {
///     println!("This is a Python project");
/// }
/// ```
pub fn is_python_project(dir: &Path) -> bool {
    ProjectFacts::detect(dir).has_python_project()
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
/// use vm_config::detector::is_pipx_environment;
///
/// let env_dir = Path::new("/home/user/.local/share/pipx/venvs/myapp");
/// if is_pipx_environment(env_dir) {
///     println!("This is a pipx environment");
/// }
/// ```
pub fn is_pipx_environment(path: &Path) -> bool {
    path.join("pipx_metadata.json").exists()
}

/// Helper function to detect JavaScript framework from package.json dependencies
fn detect_js_framework(json: &Value) -> String {
    let deps = json.get("dependencies").and_then(Value::as_object);
    let dev_deps = json.get("devDependencies").and_then(Value::as_object);

    let all_deps = deps.into_iter().chain(dev_deps).flat_map(|o| o.keys());

    for dep in all_deps {
        match dep.as_str() {
            "react" => return "react".to_string(),
            "vue" => return "vue".to_string(),
            "next" => return "next".to_string(),
            "@angular/core" => return "angular".to_string(),
            _ => continue,
        }
    }

    "nodejs".to_string()
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
/// use vm_config::detector::detect_project_type;
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
    let facts = ProjectFacts::detect(dir);

    // --- Node.js Detection ---
    if facts.package_json {
        if let Ok(content) = fs::read_to_string(dir.join("package.json")) {
            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                let framework = detect_js_framework(&json);
                types.insert(framework);
            }
            // If JSON parsing fails, we don't add nodejs type (graceful degradation)
        }
    }

    // --- Python Detection ---
    if facts.has_python_project() {
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
    if facts.cargo_toml {
        types.insert("rust".to_string());
    }

    // --- Go Detection ---
    if facts.go_mod {
        types.insert("go".to_string());
    }

    // --- Ruby Detection ---
    if facts.gemfile {
        let mut framework = "ruby".to_string();
        if has_file_containing(dir, "Gemfile", "rails") {
            framework = "rails".to_string();
        }
        types.insert(framework);
    }

    // --- PHP Detection ---
    if facts.composer_json {
        types.insert("php".to_string());
    }

    // --- Docker Detection ---
    if facts.docker {
        types.insert("docker".to_string());
    }

    // --- Kubernetes Detection ---
    if facts.kubernetes {
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
/// use vm_config::detector::format_detected_types;
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
// (Now using shared utilities from vm_core)

/// Attempts to detect the project name based on the current directory.
///
/// It takes the last component of the current working directory's path
/// and returns it as a string. This serves as a sensible default for
/// the project name.
///
/// # Returns
/// A `Result` containing the detected project name or an error if
/// the current directory cannot be determined or processed.
pub fn detect_project_name() -> Result<String> {
    let current_dir = env::current_dir().map_err(VmError::Io)?;

    let project_name = current_dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            VmError::Internal("Could not determine project name from current directory".to_string())
        })?;

    Ok(project_name)
}

pub fn detect_framework() -> Option<String> {
    let detected_types = detect_project_type(Path::new("."));
    if detected_types.is_empty() {
        None
    } else {
        Some(format_detected_types(detected_types))
    }
}

/// Detects git worktrees for the current project.
pub fn detect_worktrees() -> Result<Vec<String>> {
    detect_worktrees_in(Path::new("."))
}

/// Detects git worktrees for the project rooted at `workspace_root`.
pub fn detect_worktrees_in(workspace_root: &Path) -> Result<Vec<String>> {
    let git_dir = workspace_root.join(".git");
    let worktrees_dir = git_dir.join("worktrees");

    if !worktrees_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut worktree_paths = Vec::new();
    for entry in fs::read_dir(worktrees_dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                warn!("Skipping unreadable worktree entry: {e}");
                continue;
            }
        };

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let gitdir_path = path.join("gitdir");
        if !gitdir_path.is_file() {
            continue;
        }

        let gitdir_content = match fs::read_to_string(&gitdir_path) {
            Ok(content) => content,
            Err(e) => {
                warn!(
                    "Skipping worktree with unreadable gitdir file '{}': {e}",
                    gitdir_path.display()
                );
                continue;
            }
        };

        let worktree_path = std::path::PathBuf::from(gitdir_content.trim());
        let parent_path = match worktree_path.parent() {
            Some(parent) => parent,
            None => {
                warn!(
                    "Skipping worktree with invalid gitdir path '{}'",
                    worktree_path.display()
                );
                continue;
            }
        };

        let absolute_path = match workspace_root.join(parent_path).canonicalize() {
            Ok(absolute_path) => absolute_path,
            Err(e) => {
                warn!(
                    "Skipping worktree with invalid path '{}': {e}",
                    parent_path.display()
                );
                continue;
            }
        };

        worktree_paths.push(absolute_path.to_string_lossy().into_owned());
    }

    Ok(worktree_paths)
}

#[cfg(test)]
mod worktree_tests {
    use super::detect_worktrees_in;

    #[test]
    fn detects_worktrees_from_the_configured_project_root() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("project");
        let worktree = directory.path().join("worktree");
        let metadata = project.join(".git/worktrees/feature");
        std::fs::create_dir_all(&metadata).unwrap();
        std::fs::create_dir(&worktree).unwrap();
        std::fs::write(
            metadata.join("gitdir"),
            worktree.join(".git").to_string_lossy().as_bytes(),
        )
        .unwrap();

        assert_eq!(
            detect_worktrees_in(&project).unwrap(),
            vec![worktree.canonicalize().unwrap().to_string_lossy()]
        );
    }
}

#[cfg(test)]
mod tests;
