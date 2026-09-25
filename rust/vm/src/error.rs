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
    exit_code: i32,
    guest_exit: bool,
    reported: bool,
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
            exit_code: 1,
            guest_exit: false,
            reported: false,
        }
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    pub fn is_guest_exit(&self) -> bool {
        self.guest_exit
    }

    pub fn is_reported(&self) -> bool {
        self.reported
    }

    pub fn reported(mut self) -> Self {
        self.reported = true;
        self
    }

    pub fn guest_exit(code: i32) -> Self {
        Self {
            message: format!("Guest command exited with status {code}"),
            hint: None,
            source: None,
            exit_code: code,
            guest_exit: true,
            reported: false,
        }
    }

    pub fn exec_prelaunch(mut self) -> Self {
        self.exit_code = 125;
        self
    }

    pub fn code(&self) -> &'static str {
        match self.exit_code {
            2 => "invalid_request",
            3 => "state_conflict",
            4 => "authorization_failed",
            5 => "deadline_exceeded",
            125 => "exec_prelaunch_failed",
            _ => "operation_failed",
        }
    }

    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    pub fn source_chain(&self) -> Option<String> {
        let mut source = self.source();
        let mut messages = Vec::new();
        while let Some(error) = source {
            messages.push(clean_diagnostic(&error.to_string()));
            if messages.len() == 8 {
                break;
            }
            source = error.source();
        }
        (!messages.is_empty()).then(|| messages.join(": "))
    }

    pub fn config<E>(source: E, context: impl Into<String>) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        let mut error =
            Self::with_source(format!("Configuration error: {}", context.into()), source);
        error.exit_code = 2;
        error
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
            exit_code: 2,
            guest_exit: false,
            reported: false,
        }
    }

    pub fn conflict(message: impl Into<String>, hint: Option<impl Into<String>>) -> Self {
        Self {
            message: format!("Conflict: {}", message.into()),
            hint: hint.map(Into::into),
            source: None,
            exit_code: 3,
            guest_exit: false,
            reported: false,
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
        let message = error.to_string();
        Self {
            message,
            hint: None,
            source: Some(error.into_boxed_dyn_error()),
            exit_code: 1,
            guest_exit: false,
            reported: false,
        }
    }
}

impl From<std::io::Error> for VmError {
    fn from(error: std::io::Error) -> Self {
        let message = error.to_string();
        Self::with_source(message, error)
    }
}

impl From<dialoguer::Error> for VmError {
    fn from(error: dialoguer::Error) -> Self {
        Self::with_source("Failed to read user selection", error)
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
        let exit_code = match &error {
            vm_core::error::VmError::Config(_)
            | vm_core::error::VmError::Validation { .. }
            | vm_core::error::VmError::Serialization(_) => 2,
            vm_core::error::VmError::Conflict(_) | vm_core::error::VmError::NotFound(_) => 3,
            vm_core::error::VmError::Timeout(_) => 5,
            _ => 1,
        };
        let message = error.to_string();
        let hint = error.hint().map(str::to_string);
        Self {
            message,
            hint,
            source: Some(Box::new(error)),
            exit_code,
            guest_exit: false,
            reported: false,
        }
    }
}

fn clean_diagnostic(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(512)
        .collect::<String>()
        .trim()
        .to_string()
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
        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn exit_codes_distinguish_conflicts_and_deadlines() {
        assert_eq!(VmError::conflict("stopped", None::<String>).exit_code(), 3);
        assert_eq!(
            VmError::from(vm_core::error::VmError::Timeout("waited".into())).exit_code(),
            5
        );
        assert_eq!(
            VmError::from(vm_core::error::VmError::NotFound("environment".into())).exit_code(),
            3
        );
    }

    #[test]
    fn io_error_surfaces_the_underlying_message() {
        let error = VmError::from(io::Error::new(io::ErrorKind::Other, "connection closed"));

        assert_eq!(error.to_string(), "connection closed");
        assert_eq!(error.source().unwrap().to_string(), "connection closed");
    }

    #[test]
    fn conversions_preserve_hints_and_error_chains() {
        let validation = VmError::from(vm_core::error::VmError::validation(
            "invalid target",
            Some("select one environment"),
        ));
        assert_eq!(validation.hint(), Some("select one environment"));
        assert!(validation.source().is_some());

        let source = io::Error::new(io::ErrorKind::Other, "disk unavailable");
        let anyhow_error = anyhow::Error::new(source).context("could not save state");
        let converted = VmError::from(anyhow_error);
        assert_eq!(converted.to_string(), "could not save state");
        assert!(converted
            .source_chain()
            .is_some_and(|chain| chain.contains("disk unavailable")));
    }
}
