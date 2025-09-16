use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use vm_common::file_system::{has_any_file, has_file, has_file_containing, has_any_dir};

pub mod os;
pub mod presets;
pub mod tools;

pub use os::detect_host_os;
pub use presets::{detect_preset_for_project, is_react_project, get_recommended_preset, is_multi_tech_project, get_detected_technologies};
pub use tools::{ToolDetector, has_command, detect_languages, detect_databases};

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
mod tests;
