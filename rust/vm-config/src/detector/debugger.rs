/// Debugger port detection and configuration.
///
/// This module provides functionality to automatically detect appropriate
/// debugger ports based on project type when debugging is enabled.
use super::detect_project_type;
use std::path::Path;

/// Standard debugger ports for different languages and frameworks
#[derive(Debug, Clone, PartialEq)]
pub struct DebuggerPort {
    pub port: u16,
    pub protocol: String,
    pub description: String,
}

/// Get debugger ports for detected project types
///
/// Returns a list of debugger ports that should be exposed based on
/// the detected technologies in the project directory.
///
/// # Arguments
/// * `project_dir` - The project directory to analyze
///
/// # Returns
/// Vector of (port, description) tuples for debuggers that should be exposed
pub fn detect_debugger_ports(project_dir: &Path) -> Vec<DebuggerPort> {
    let detected_types = detect_project_type(project_dir);
    let mut ports = Vec::new();

    // Node.js / JavaScript debugger (Inspector Protocol)
    if detected_types.contains("nodejs")
        || detected_types.contains("next")
        || detected_types.contains("react")
        || detected_types.contains("vue")
        || detected_types.contains("angular")
    {
        ports.push(DebuggerPort {
            port: 9229,
            protocol: "Inspector Protocol".to_string(),
            description: "Node.js debugger".to_string(),
        });
    }

    // Python debugger (debugpy)
    if detected_types.contains("python")
        || detected_types.contains("django")
        || detected_types.contains("flask")
    {
        ports.push(DebuggerPort {
            port: 5678,
            protocol: "DAP".to_string(),
            description: "Python debugger (debugpy)".to_string(),
        });
    }

    // Go debugger (Delve)
    if detected_types.contains("go") {
        ports.push(DebuggerPort {
            port: 2345,
            protocol: "DAP".to_string(),
            description: "Go debugger (Delve)".to_string(),
        });
    }

    // PHP debugger (Xdebug 3.x)
    if detected_types.contains("php") {
        ports.push(DebuggerPort {
            port: 9003,
            protocol: "DBGp".to_string(),
            description: "PHP debugger (Xdebug)".to_string(),
        });
    }

    // Ruby debugger (ruby-debug-ide / rdbg)
    if detected_types.contains("ruby") || detected_types.contains("rails") {
        ports.push(DebuggerPort {
            port: 1234,
            protocol: "DAP".to_string(),
            description: "Ruby debugger".to_string(),
        });
    }

    // Java debugger (JDWP)
    if detected_types.contains("java") {
        ports.push(DebuggerPort {
            port: 5005,
            protocol: "JDWP".to_string(),
            description: "Java debugger".to_string(),
        });
    }

    // Rust doesn't have a standard network debugger port
    // (typically uses gdb/lldb with local protocols)

    ports
}

/// Get all supported debugger configurations
///
/// Returns a map of language/framework to their standard debugger ports.
/// Useful for documentation and reference.
pub fn get_all_debugger_ports() -> Vec<(&'static str, DebuggerPort)> {
    vec![
        (
            "nodejs",
            DebuggerPort {
                port: 9229,
                protocol: "Inspector Protocol".to_string(),
                description: "Node.js debugger".to_string(),
            },
        ),
        (
            "python",
            DebuggerPort {
                port: 5678,
                protocol: "DAP".to_string(),
                description: "Python debugger (debugpy)".to_string(),
            },
        ),
        (
            "go",
            DebuggerPort {
                port: 2345,
                protocol: "DAP".to_string(),
                description: "Go debugger (Delve)".to_string(),
            },
        ),
        (
            "php",
            DebuggerPort {
                port: 9003,
                protocol: "DBGp".to_string(),
                description: "PHP debugger (Xdebug)".to_string(),
            },
        ),
        (
            "ruby",
            DebuggerPort {
                port: 1234,
                protocol: "DAP".to_string(),
                description: "Ruby debugger".to_string(),
            },
        ),
        (
            "java",
            DebuggerPort {
                port: 5005,
                protocol: "JDWP".to_string(),
                description: "Java debugger".to_string(),
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_nodejs_debugger_detection() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();

        let ports = detect_debugger_ports(temp_dir.path());
        assert!(!ports.is_empty());
        assert!(ports.iter().any(|p| p.port == 9229));
    }

    #[test]
    fn test_python_debugger_detection() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("requirements.txt"), "django\n").unwrap();

        let ports = detect_debugger_ports(temp_dir.path());
        assert!(!ports.is_empty());
        assert!(ports.iter().any(|p| p.port == 5678));
    }

    #[test]
    fn test_no_debugger_for_unknown_project() {
        let temp_dir = TempDir::new().unwrap();
        // Empty directory

        let ports = detect_debugger_ports(temp_dir.path());
        assert!(ports.is_empty());
    }

    #[test]
    fn test_multi_language_project() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("package.json"), r#"{"name": "test"}"#).unwrap();
        fs::write(temp_dir.path().join("requirements.txt"), "flask\n").unwrap();

        let ports = detect_debugger_ports(temp_dir.path());
        assert!(ports.len() >= 2);
        assert!(ports.iter().any(|p| p.port == 9229)); // Node.js
        assert!(ports.iter().any(|p| p.port == 5678)); // Python
    }
}
