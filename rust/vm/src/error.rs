//! Error types for the VM CLI application.
//!
//! This module provides a centralized error handling system with user-friendly
//! error messages and proper error categorization.

use std::error::Error;
use std::fmt;

/// Primary error type for the VM CLI application.
///
/// This enum categorizes all possible errors that can occur during VM operations,
/// providing context-specific error handling and user-friendly error messages.
#[derive(Debug)]
#[allow(dead_code)]
pub enum VmError {
    /// Configuration-related errors
    Config {
        /// The specific configuration error
        source: Box<dyn std::error::Error + Send + Sync>,
        /// Additional context about what was being configured
        context: String,
    },

    /// Provider-related errors (Docker, etc.)
    Provider {
        /// The specific provider error
        source: Box<dyn std::error::Error + Send + Sync>,
        /// The provider type that failed
        provider_type: String,
        /// Additional context about the operation
        context: String,
    },

    /// Authentication/secrets management errors
    Auth {
        /// The specific auth error
        source: Box<dyn std::error::Error + Send + Sync>,
        /// Additional context about the auth operation
        context: String,
    },

    /// Package management errors
    Package {
        /// The specific package error
        source: Box<dyn std::error::Error + Send + Sync>,
        /// The package name if applicable
        package_name: Option<String>,
        /// Additional context about the package operation
        context: String,
    },

    /// Docker registry errors
    Registry {
        /// The specific registry error
        source: Box<dyn std::error::Error + Send + Sync>,
        /// The registry URL if applicable
        registry_url: Option<String>,
        /// Additional context about the registry operation
        context: String,
    },

    /// VM lifecycle operation errors
    VmOperation {
        /// The specific VM operation error
        source: Box<dyn std::error::Error + Send + Sync>,
        /// The VM name if applicable
        vm_name: Option<String>,
        /// The operation that failed
        operation: String,
    },

    /// File system related errors
    FileSystem {
        /// The specific filesystem error
        source: Box<dyn std::error::Error + Send + Sync>,
        /// The file path that caused the error
        path: String,
        /// The operation that failed
        operation: String,
    },

    /// Network/HTTP related errors
    Network {
        /// The specific network error
        source: Box<dyn std::error::Error + Send + Sync>,
        /// The URL or endpoint that failed
        endpoint: Option<String>,
        /// Additional context about the network operation
        context: String,
    },

    /// Validation errors for user input
    Validation {
        /// Description of what validation failed
        message: String,
        /// The field or input that failed validation
        field: Option<String>,
    },

    /// General application errors that don't fit other categories
    General {
        /// The underlying error
        source: Box<dyn std::error::Error + Send + Sync>,
        /// Additional context about the error
        context: String,
    },
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::Config { context, .. } => {
                write!(f, "Configuration error: {context}")
            }
            VmError::Provider {
                provider_type,
                context,
                ..
            } => {
                write!(f, "Provider error ({provider_type}): {context}")
            }
            VmError::Auth { context, .. } => {
                write!(f, "Authentication error: {context}")
            }
            VmError::Package {
                package_name,
                context,
                ..
            } => match package_name {
                Some(name) => write!(f, "Package error for '{name}': {context}"),
                None => write!(f, "Package error: {context}"),
            },
            VmError::Registry {
                registry_url,
                context,
                ..
            } => match registry_url {
                Some(url) => write!(f, "Registry error for '{url}': {context}"),
                None => write!(f, "Registry error: {context}"),
            },
            VmError::VmOperation {
                vm_name, operation, ..
            } => match vm_name {
                Some(name) => write!(
                    f,
                    "VM operation '{}' failed for '{}': {}",
                    operation,
                    name,
                    self.source_message()
                ),
                None => write!(
                    f,
                    "VM operation '{}' failed: {}",
                    operation,
                    self.source_message()
                ),
            },
            VmError::FileSystem {
                path, operation, ..
            } => {
                write!(
                    f,
                    "Filesystem error during '{}' on '{}': {}",
                    operation,
                    path,
                    self.source_message()
                )
            }
            VmError::Network {
                endpoint, context, ..
            } => match endpoint {
                Some(url) => write!(f, "Network error connecting to '{url}': {context}"),
                None => write!(f, "Network error: {context}"),
            },
            VmError::Validation { message, field } => match field {
                Some(field_name) => write!(f, "Validation error for '{field_name}': {message}"),
                None => write!(f, "Validation error: {message}"),
            },
            VmError::General { context, .. } => {
                write!(f, "Error: {context}")
            }
        }
    }
}

impl std::error::Error for VmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VmError::Config { source, .. }
            | VmError::Provider { source, .. }
            | VmError::Auth { source, .. }
            | VmError::Package { source, .. }
            | VmError::Registry { source, .. }
            | VmError::VmOperation { source, .. }
            | VmError::FileSystem { source, .. }
            | VmError::Network { source, .. }
            | VmError::General { source, .. } => Some(source.as_ref()),
            VmError::Validation { .. } => None,
        }
    }
}

#[allow(dead_code)]
impl VmError {
    /// Get the source error message if available
    fn source_message(&self) -> String {
        match self.source() {
            Some(source) => source.to_string(),
            None => "Unknown error".to_string(),
        }
    }

    /// Create a configuration error
    pub fn config<E: std::error::Error + Send + Sync + 'static>(
        source: E,
        context: impl Into<String>,
    ) -> Self {
        Self::Config {
            source: Box::new(source),
            context: context.into(),
        }
    }

    /// Create a provider error
    pub fn provider<E: std::error::Error + Send + Sync + 'static>(
        source: E,
        provider_type: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        Self::Provider {
            source: Box::new(source),
            provider_type: provider_type.into(),
            context: context.into(),
        }
    }

    /// Create an authentication error
    pub fn auth<E: std::error::Error + Send + Sync + 'static>(
        source: E,
        context: impl Into<String>,
    ) -> Self {
        Self::Auth {
            source: Box::new(source),
            context: context.into(),
        }
    }

    /// Create a package error
    pub fn package<E: std::error::Error + Send + Sync + 'static>(
        source: E,
        package_name: Option<impl Into<String>>,
        context: impl Into<String>,
    ) -> Self {
        Self::Package {
            source: Box::new(source),
            package_name: package_name.map(|s| s.into()),
            context: context.into(),
        }
    }

    /// Create a registry error
    pub fn registry<E: std::error::Error + Send + Sync + 'static>(
        source: E,
        registry_url: Option<impl Into<String>>,
        context: impl Into<String>,
    ) -> Self {
        Self::Registry {
            source: Box::new(source),
            registry_url: registry_url.map(|s| s.into()),
            context: context.into(),
        }
    }

    /// Create a VM operation error
    pub fn vm_operation<E: std::error::Error + Send + Sync + 'static>(
        source: E,
        vm_name: Option<impl Into<String>>,
        operation: impl Into<String>,
    ) -> Self {
        Self::VmOperation {
            source: Box::new(source),
            vm_name: vm_name.map(|s| s.into()),
            operation: operation.into(),
        }
    }

    /// Create a filesystem error
    pub fn filesystem<E: std::error::Error + Send + Sync + 'static>(
        source: E,
        path: impl Into<String>,
        operation: impl Into<String>,
    ) -> Self {
        Self::FileSystem {
            source: Box::new(source),
            path: path.into(),
            operation: operation.into(),
        }
    }

    /// Create a network error
    pub fn network<E: std::error::Error + Send + Sync + 'static>(
        source: E,
        endpoint: Option<impl Into<String>>,
        context: impl Into<String>,
    ) -> Self {
        Self::Network {
            source: Box::new(source),
            endpoint: endpoint.map(|s| s.into()),
            context: context.into(),
        }
    }

    /// Create a validation error
    pub fn validation(message: impl Into<String>, field: Option<impl Into<String>>) -> Self {
        Self::Validation {
            message: message.into(),
            field: field.map(|s| s.into()),
        }
    }

    /// Create a general error
    pub fn general<E: std::error::Error + Send + Sync + 'static>(
        source: E,
        context: impl Into<String>,
    ) -> Self {
        Self::General {
            source: Box::new(source),
            context: context.into(),
        }
    }
}

/// Convenience type alias for Results using VmError
#[allow(dead_code)]
pub type VmResult<T> = Result<T, VmError>;

/// Convert from anyhow::Error to VmError
impl From<anyhow::Error> for VmError {
    fn from(err: anyhow::Error) -> Self {
        // Preserve the actual error message in the context field
        // This ensures users see meaningful error messages instead of "An error occurred"
        let error_msg = err.to_string();
        VmError::General {
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                error_msg.clone(),
            )),
            context: error_msg,
        }
    }
}

/// Convert from std::io::Error to VmError
impl From<std::io::Error> for VmError {
    fn from(err: std::io::Error) -> Self {
        VmError::General {
            source: Box::new(err),
            context: "I/O error occurred".to_string(),
        }
    }
}

/// Convert from vm_core::error::VmError to VmError
impl From<vm_core::error::VmError> for VmError {
    fn from(err: vm_core::error::VmError) -> Self {
        match err {
            vm_core::error::VmError::Config(msg) => VmError::Config {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                context: msg,
            },
            vm_core::error::VmError::Provider(msg) => VmError::Provider {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                provider_type: "vm-provider".to_string(),
                context: msg,
            },
            vm_core::error::VmError::Command(msg) => VmError::VmOperation {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                vm_name: None,
                operation: "command execution".to_string(),
            },
            vm_core::error::VmError::Dependency(msg) => VmError::General {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                context: format!("Dependency error: {msg}"),
            },
            vm_core::error::VmError::Network(msg) => VmError::Network {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                endpoint: None,
                context: msg,
            },
            vm_core::error::VmError::Internal(msg) => VmError::General {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                context: format!("Internal error: {msg}"),
            },
            vm_core::error::VmError::Filesystem(msg) => VmError::FileSystem {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                path: "unknown".to_string(),
                operation: "filesystem operation".to_string(),
            },
            vm_core::error::VmError::Serialization(msg) => VmError::General {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                context: format!("Serialization error: {msg}"),
            },
            vm_core::error::VmError::Migration(msg) => VmError::General {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                context: format!("Migration error: {msg}"),
            },
            vm_core::error::VmError::DockerNotInstalled(msg) => VmError::General {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                context: format!("Docker not installed: {msg}"),
            },
            vm_core::error::VmError::DockerNotRunning(msg) => VmError::General {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                context: format!("Docker not running: {msg}"),
            },
            vm_core::error::VmError::DockerPermission(msg) => VmError::General {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                context: format!("Docker permission error: {msg}"),
            },
            vm_core::error::VmError::NotFound(msg) => VmError::General {
                source: Box::new(std::io::Error::new(std::io::ErrorKind::Other, msg.clone())),
                context: msg,
            },
            vm_core::error::VmError::Io(err) => VmError::from(err),
            vm_core::error::VmError::Other(err) => VmError::from(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_config_error_display() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "config file not found");
        let vm_err = VmError::config(io_err, "Failed to load configuration");

        assert_eq!(
            vm_err.to_string(),
            "Configuration error: Failed to load configuration"
        );
    }

    #[test]
    fn test_vm_operation_error_with_name() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "permission denied");
        let vm_err = VmError::vm_operation(io_err, Some("my-vm"), "start");

        assert!(vm_err
            .to_string()
            .contains("VM operation 'start' failed for 'my-vm'"));
    }

    #[test]
    fn test_validation_error() {
        let vm_err = VmError::validation("Invalid port number", Some("port"));

        assert_eq!(
            vm_err.to_string(),
            "Validation error for 'port': Invalid port number"
        );
    }

    #[test]
    fn test_package_error_with_name() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "package not found");
        let vm_err = VmError::package(io_err, Some("my-package"), "Failed to install package");

        assert_eq!(
            vm_err.to_string(),
            "Package error for 'my-package': Failed to install package"
        );
    }

    #[test]
    fn test_error_source_chain() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let vm_err = VmError::config(io_err, "Failed to read config");

        if let Some(source) = vm_err.source() {
            assert_eq!(source.to_string(), "file not found");
        } else {
            panic!("source() should not be None");
        }
    }
}
