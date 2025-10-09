//! # Input Validation: Shell Escaping
//!
//! This module provides utilities for safely escaping shell arguments to
//! prevent command injection attacks.

/// Escape a shell argument to prevent command injection attacks.
///
/// This function properly escapes shell metacharacters and quotes the argument
/// to ensure it can be safely passed to shell commands. It uses single quotes
/// for maximum safety and handles embedded single quotes correctly.
///
/// # Arguments
///
/// * `arg` - The argument to escape
///
/// # Returns
///
/// A safely escaped string that can be used in shell commands
///
/// # Examples
///
/// ```rust
/// use vm_package_server::validation::shell::escape_shell_arg;
///
/// let safe = escape_shell_arg("file with spaces.txt");
/// assert_eq!(safe, "'file with spaces.txt'");
///
/// let safe = escape_shell_arg("file'with'quotes.txt");
/// assert_eq!(safe, "'file'\\''with'\\''quotes.txt'");
/// ```
pub fn escape_shell_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }

    // Check for null bytes
    if arg.contains('\0') {
        // Return empty quoted string for safety
        return "''".to_string();
    }

    // If the argument contains only safe characters, return it as-is
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
    {
        return arg.to_string();
    }

    // Otherwise, wrap in single quotes and escape any embedded single quotes
    format!("'{}'", arg.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_shell_arg() {
        assert_eq!(escape_shell_arg("simple"), "simple");
        assert_eq!(escape_shell_arg("file with spaces"), "'file with spaces'");
        assert_eq!(
            escape_shell_arg("file'with'quotes"),
            "'file'\\''with'\\''quotes'"
        );
        assert_eq!(escape_shell_arg(""), "''");
        assert_eq!(escape_shell_arg("file\0with\0nulls"), "''");
    }

    #[test]
    fn test_escape_shell_arg_for_docker() {
        // Docker arguments are escaped using the same shell logic
        assert_eq!(escape_shell_arg("simple"), "simple");
        assert_eq!(escape_shell_arg("path with spaces"), "'path with spaces'");
        assert_eq!(
            escape_shell_arg("dangerous$variable"),
            "'dangerous$variable'"
        );
        assert_eq!(
            escape_shell_arg("command`injection`"),
            "'command`injection`'"
        );
    }
}