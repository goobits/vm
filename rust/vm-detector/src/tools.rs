use which::which;

/// Tool and runtime detection utilities.
///
/// Provides methods for detecting installed development tools, language runtimes,
/// and database systems on the host system. This is useful for:
/// - Verifying development environment requirements
/// - Providing installation recommendations
/// - Adapting VM configurations based on available tools
///
/// All detection is performed by checking if commands are available in the system PATH.
pub struct ToolDetector;

impl ToolDetector {
    /// Check if a command is available in the system PATH.
    ///
    /// This is the core detection method used by other detection functions.
    /// It verifies whether a given command/executable can be found and executed.
    ///
    /// # Arguments
    /// * `cmd` - The command name to check for (e.g., "node", "python", "git")
    ///
    /// # Returns
    /// `true` if the command is found in PATH, `false` otherwise
    ///
    /// # Examples
    /// ```rust
    /// use vm_detector::ToolDetector;
    ///
    /// if ToolDetector::has_command("git") {
    ///     println!("Git is installed");
    /// }
    /// ```
    pub fn has_command(cmd: &str) -> bool {
        which(cmd).is_ok()
    }

    /// Detect installed programming language runtimes.
    ///
    /// Scans the system for common programming language runtimes and development tools.
    /// This helps determine what languages can be used in the development environment.
    ///
    /// ## Detected Languages
    /// - **nodejs** - Node.js runtime (checks for `node` or `npm`)
    /// - **python** - Python interpreter (checks for `python` or `python3`)
    /// - **ruby** - Ruby interpreter (checks for `ruby` or `gem`)
    /// - **rust** - Rust toolchain (checks for `cargo` or `rustc`)
    /// - **go** - Go compiler and runtime (checks for `go`)
    /// - **java** - Java runtime and compiler (checks for `java` or `javac`)
    ///
    /// # Returns
    /// A vector of detected language identifiers (may be empty)
    ///
    /// # Examples
    /// ```rust
    /// use vm_detector::ToolDetector;
    ///
    /// let languages = ToolDetector::detect_languages();
    /// for lang in languages {
    ///     println!("Found language runtime: {}", lang);
    /// }
    /// ```
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

    /// Detect installed database systems.
    ///
    /// Scans the system for common database clients and tools. This helps
    /// determine what databases are available for development and testing.
    ///
    /// ## Detected Databases
    /// - **postgresql** - PostgreSQL database (checks for `psql` client)
    /// - **mysql** - MySQL database (checks for `mysql` client)
    /// - **mongodb** - MongoDB database (checks for `mongosh` or legacy `mongo` client)
    /// - **redis** - Redis key-value store (checks for `redis-cli` client)
    ///
    /// # Returns
    /// A vector of detected database identifiers (may be empty)
    ///
    /// # Examples
    /// ```rust
    /// use vm_detector::ToolDetector;
    ///
    /// let databases = ToolDetector::detect_databases();
    /// if databases.contains(&"postgresql".to_string()) {
    ///     println!("PostgreSQL is available");
    /// }
    /// ```
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

/// Check if a command is available in PATH (convenience function).
///
/// This is a module-level convenience function that wraps `ToolDetector::has_command`
/// for easier use without needing to reference the struct.
///
/// # Arguments
/// * `cmd` - The command name to check for
///
/// # Returns
/// `true` if the command is found in PATH, `false` otherwise
///
/// # Examples
/// ```rust
/// use vm_detector::has_command;
///
/// if has_command("docker") {
///     println!("Docker is available");
/// }
/// ```
pub fn has_command(cmd: &str) -> bool {
    ToolDetector::has_command(cmd)
}

/// Detect installed language runtimes (convenience function).
///
/// This is a module-level convenience function that wraps `ToolDetector::detect_languages`
/// for easier use without needing to reference the struct.
///
/// # Returns
/// A vector of detected language runtime identifiers
///
/// # Examples
/// ```rust
/// use vm_detector::detect_languages;
///
/// let languages = detect_languages();
/// println!("Available languages: {:?}", languages);
/// ```
pub fn detect_languages() -> Vec<String> {
    ToolDetector::detect_languages()
}

/// Detect installed databases (convenience function).
///
/// This is a module-level convenience function that wraps `ToolDetector::detect_databases`
/// for easier use without needing to reference the struct.
///
/// # Returns
/// A vector of detected database system identifiers
///
/// # Examples
/// ```rust
/// use vm_detector::detect_databases;
///
/// let databases = detect_databases();
/// for db in databases {
///     println!("Found database: {}", db);
/// }
/// ```
pub fn detect_databases() -> Vec<String> {
    ToolDetector::detect_databases()
}

/// Check if a directory contains a Python project (legacy function).
///
/// **Note**: This function is deprecated in favor of the main `is_python_project`
/// function in the root module, which provides more comprehensive detection.
///
/// This function only checks for `setup.py` and `pyproject.toml` files.
/// The main function also checks for `requirements.txt` and `Pipfile`.
///
/// # Arguments
/// * `path` - The directory path to check
///
/// # Returns
/// `true` if Python project files are found, `false` otherwise
///
/// # Examples
/// ```rust
/// use std::path::Path;
/// use vm_detector::tools::is_python_project;
///
/// let project_dir = Path::new("/path/to/python/project");
/// if is_python_project(project_dir) {
///     println!("Found Python project files");
/// }
/// ```
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
