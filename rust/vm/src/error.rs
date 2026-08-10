//! CLI error context and presentation metadata.

use std::error::Error;
use std::fmt;

type ErrorSource = Box<dyn Error + Send + Sync>;

/// An actionable CLI error with an optional follow-up hint.
#[derive(Debug)]
pub struct VmError {
    message: String,
    hint: Option<String>,
    source: Option<ErrorSource>,
}

impl VmError {
    fn with_source<E>(message: impl Into<String>, source: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            message: message.into(),
            hint: None,
            source: Some(Box::new(source)),
        }
    }

    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: None,
            source: None,
        }
    }

    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    pub fn config<E>(source: E, context: impl Into<String>) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self::with_source(format!("Configuration error: {}", context.into()), source)
    }

    pub fn vm_operation<E>(
        source: E,
        vm_name: Option<impl Into<String>>,
        operation: impl Into<String>,
    ) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        let operation = operation.into();
        let source_message = source.to_string();
        let message = vm_name.map_or_else(
            || format!("VM operation '{operation}' failed: {source_message}"),
            |name| {
                format!(
                    "VM operation '{operation}' failed for '{}': {source_message}",
                    name.into()
                )
            },
        );
        Self::with_source(message, source)
    }

    pub fn filesystem<E>(source: E, path: impl Into<String>, operation: impl Into<String>) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        let path = path.into();
        let operation = operation.into();
        let message = format!(
            "Filesystem error during '{operation}' on '{path}': {}",
            source
        );
        Self::with_source(message, source)
    }

    pub fn validation(message: impl Into<String>, hint: Option<impl Into<String>>) -> Self {
        Self {
            message: format!("Validation error: {}", message.into()),
            hint: hint.map(Into::into),
            source: None,
        }
    }

    pub fn general<E>(source: E, context: impl Into<String>) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self::with_source(context, source)
    }
}

impl fmt::Display for VmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for VmError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as _)
    }
}

pub type VmResult<T> = Result<T, VmError>;

impl From<anyhow::Error> for VmError {
    fn from(error: anyhow::Error) -> Self {
        Self::message(error.to_string())
    }
}

impl From<std::io::Error> for VmError {
    fn from(error: std::io::Error) -> Self {
        let message = error.to_string();
        Self::with_source(message, error)
    }
}

impl From<serde_json::Error> for VmError {
    fn from(error: serde_json::Error) -> Self {
        Self::with_source("Invalid package infrastructure metadata", error)
    }
}

impl From<vm_packages::PackageValidationError> for VmError {
    fn from(error: vm_packages::PackageValidationError) -> Self {
        Self::validation(error.to_string(), None::<String>)
    }
}

impl From<vm_core::error::VmError> for VmError {
    fn from(error: vm_core::error::VmError) -> Self {
        Self::message(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn configuration_error_keeps_context_and_source() {
        let error = VmError::config(
            io::Error::new(io::ErrorKind::NotFound, "file not found"),
            "Failed to read config",
        );

        assert_eq!(
            error.to_string(),
            "Configuration error: Failed to read config"
        );
        assert_eq!(error.source().unwrap().to_string(), "file not found");
    }

    #[test]
    fn vm_operation_error_names_target() {
        let error = VmError::vm_operation(
            io::Error::new(io::ErrorKind::PermissionDenied, "permission denied"),
            Some("my-vm"),
            "start",
        );

        assert_eq!(
            error.to_string(),
            "VM operation 'start' failed for 'my-vm': permission denied"
        );
    }

    #[test]
    fn validation_error_exposes_hint_separately() {
        let error = VmError::validation("Invalid port number", Some("Use a port from 1-65535"));

        assert_eq!(error.to_string(), "Validation error: Invalid port number");
        assert_eq!(error.hint(), Some("Use a port from 1-65535"));
        assert!(error.source().is_none());
    }
}
