use which::which;

/// Tool and runtime detection utilities
pub struct ToolDetector;

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

/// Check if a command is available in PATH (convenience function)
pub fn has_command(cmd: &str) -> bool {
    ToolDetector::has_command(cmd)
}

/// Detect installed language runtimes (convenience function)
pub fn detect_languages() -> Vec<String> {
    ToolDetector::detect_languages()
}

/// Detect installed databases (convenience function)
pub fn detect_databases() -> Vec<String> {
    ToolDetector::detect_databases()
}

/// Check if a path is a Python project (has setup.py or pyproject.toml)
pub fn is_python_project(path: &std::path::Path) -> bool {
    path.join("setup.py").exists() || path.join("pyproject.toml").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_command() {
        // Test with a command that should exist on most systems
        assert!(ToolDetector::has_command("ls") || ToolDetector::has_command("dir"));

        // Test with a command that definitely doesn't exist
        assert!(!ToolDetector::has_command(
            "definitely_does_not_exist_command_12345"
        ));
    }

    #[test]
    fn test_detect_languages() {
        let languages = ToolDetector::detect_languages();
        // Should return a vector (may be empty in test environments)
        // Vec::len() is always >= 0, this assertion is redundant but kept for clarity

        // If any languages are detected, they should be valid
        for lang in &languages {
            assert!(matches!(
                lang.as_str(),
                "nodejs" | "python" | "ruby" | "rust" | "go" | "java"
            ));
        }
    }

    #[test]
    fn test_detect_databases() {
        let databases = ToolDetector::detect_databases();
        // Should return a vector (may be empty in test environments)
        // Vec::len() is always >= 0, this assertion is redundant but kept for clarity

        // If any databases are detected, they should be valid
        for db in &databases {
            assert!(matches!(
                db.as_str(),
                "postgresql" | "mysql" | "mongodb" | "redis"
            ));
        }
    }

    #[test]
    fn test_convenience_functions() {
        // Test that convenience functions work the same as struct methods
        assert_eq!(has_command("ls"), ToolDetector::has_command("ls"));
        assert_eq!(detect_languages(), ToolDetector::detect_languages());
        assert_eq!(detect_databases(), ToolDetector::detect_databases());
    }

    #[test]
    fn test_is_python_project() {
        use std::path::Path;

        // Test with non-existent path
        assert!(!is_python_project(Path::new("/definitely/does/not/exist")));

        // Note: Testing with actual files would require creating temp files,
        // which we'll skip for this unit test
    }
}
