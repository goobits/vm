//! Shared error traits for consistent error handling across the VM tool workspace
//!
//! This module provides common traits and utilities for error handling to promote
//! consistency and interoperability between different crates in the workspace.

use std::fmt;

/// Common trait for errors that can provide user-friendly messages
pub trait UserFriendlyError {
    /// Get a user-friendly error message suitable for display in CLI output
    fn user_message(&self) -> String;

    /// Get an optional error code that can be used for programmatic error handling
    fn error_code(&self) -> Option<&str> {
        None
    }
}

/// Common trait for errors that can provide debugging context
pub trait ErrorContext {
    /// Get additional context information for debugging
    fn context(&self) -> Option<String> {
        None
    }

    /// Get the error category for classification
    fn category(&self) -> ErrorCategory {
        ErrorCategory::Other
    }
}

/// Categories of errors for better classification and handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Configuration-related errors
    Configuration,
    /// Network or I/O related errors
    Network,
    /// File system related errors
    FileSystem,
    /// Validation errors (user input, data format, etc.)
    Validation,
    /// Provider-specific errors (Docker, VM management, etc.)
    Provider,
    /// Internal application errors
    Internal,
    /// Other/uncategorized errors
    Other,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCategory::Configuration => write!(f, "Configuration"),
            ErrorCategory::Network => write!(f, "Network"),
            ErrorCategory::FileSystem => write!(f, "FileSystem"),
            ErrorCategory::Validation => write!(f, "Validation"),
            ErrorCategory::Provider => write!(f, "Provider"),
            ErrorCategory::Internal => write!(f, "Internal"),
            ErrorCategory::Other => write!(f, "Other"),
        }
    }
}

/// Helper function to format errors with consistent style
pub fn format_error_with_context<E>(error: &E, show_debug: bool) -> String
where
    E: UserFriendlyError + ErrorContext + fmt::Debug,
{
    let mut message = error.user_message();

    if let Some(code) = error.error_code() {
        message = format!("[{}] {}", code, message);
    }

    if show_debug {
        if let Some(context) = error.context() {
            message = format!("{}\nContext: {}", message, context);
        }
        message = format!("{}\nDebug: {:?}", message, error);
    }

    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestError {
        message: String,
        code: Option<String>,
    }

    impl UserFriendlyError for TestError {
        fn user_message(&self) -> String {
            self.message.clone()
        }

        fn error_code(&self) -> Option<&str> {
            self.code.as_deref()
        }
    }

    impl ErrorContext for TestError {
        fn category(&self) -> ErrorCategory {
            ErrorCategory::Validation
        }
    }

    #[test]
    fn test_user_friendly_error() {
        let error = TestError {
            message: "Test error message".to_string(),
            code: Some("TEST001".to_string()),
        };

        assert_eq!(error.user_message(), "Test error message");
        assert_eq!(error.error_code(), Some("TEST001"));
        assert_eq!(error.category(), ErrorCategory::Validation);
    }

    #[test]
    fn test_error_formatting() {
        let error = TestError {
            message: "Test error".to_string(),
            code: Some("TEST001".to_string()),
        };

        let formatted = format_error_with_context(&error, false);
        assert_eq!(formatted, "[TEST001] Test error");
    }
}
