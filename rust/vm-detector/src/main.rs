use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;
use serde_json::Value;

fn main() {
    let project_dir = env::current_dir().expect("Failed to get current directory");
    let detected_types = detect_project_type(&project_dir);
    let mut sorted_types: Vec<String> = detected_types.into_iter().collect();
    sorted_types.sort(); // Sort for deterministic output

    if sorted_types.is_empty() {
        println!("generic");
    } else if sorted_types.len() == 1 {
        println!("{}", sorted_types[0]);
    } else {
        println!("multi:{}", sorted_types.join(" "));
    }
}

fn detect_project_type(dir: &Path) -> HashSet<String> {
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
                        "react" => { framework = "react".to_string(); break; }
                        "vue" => { framework = "vue".to_string(); break; }
                        "next" => { framework = "next".to_string(); break; }
                        "@angular/core" => { framework = "angular".to_string(); break; }
                        _ => (),
                    }
                }
                types.insert(framework);
            }
        }
    }

    // --- Python Detection ---
    if has_any_file(dir, &["requirements.txt", "pyproject.toml", "setup.py", "Pipfile"]) {
        let mut framework = "python".to_string();
        if has_file_containing(dir, "requirements.txt", "django") {
            framework = "django".to_string();
        } else if has_file_containing(dir, "requirements.txt", "flask") {
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
    if has_any_file(dir, &["Dockerfile", "docker-compose.yml", "docker-compose.yaml"]) {
        types.insert("docker".to_string());
    }
    
    // --- Kubernetes Detection ---
    if has_any_file(dir, &["k8s.yaml", "k8s.yml"]) || has_any_dir(dir, &["kubernetes", "k8s"]) {
        types.insert("kubernetes".to_string());
    }

    types
}

// --- Helper Functions ---

fn has_file(base_dir: &Path, file_name: &str) -> bool {
    base_dir.join(file_name).exists()
}

fn has_any_file(base_dir: &Path, file_names: &[&str]) -> bool {
    file_names.iter().any(|f| has_file(base_dir, f))
}

fn has_any_dir(base_dir: &Path, dir_names: &[&str]) -> bool {
    dir_names.iter().any(|d| base_dir.join(d).is_dir())
}

fn has_file_containing(base_dir: &Path, file_name: &str, pattern: &str) -> bool {
    if let Ok(content) = fs::read_to_string(base_dir.join(file_name)) {
        return content.contains(pattern);
    }
    false
}
