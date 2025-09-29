//! Tracing initialization for the VM tool
//!
//! This module provides a clean tracing setup without any legacy code.
//! It replaces the old structured_log, log_context, and module_logger systems.

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use vm_core::error::Result;

/// Initialize the tracing subscriber with environment-based configuration
///
/// Uses standard RUST_LOG environment variable for filtering:
/// - `RUST_LOG=debug` - Set global level
/// - `RUST_LOG=vm=info,vm_provider=debug` - Set per-module levels
/// - `RUST_LOG=vm[request_id=abc123]` - Filter by span fields
///
/// Uses RUST_LOG_FORMAT for output format (optional):
/// - `json` - JSON formatted output
/// - `pretty` - Pretty formatted output (default)
/// - `compact` - Compact single-line output
pub fn init() -> Result<()> {
    init_with_defaults("info")
}

/// Initialize with a default filter if RUST_LOG is not set
///
/// # Arguments
/// * `default_filter` - The filter string to use if RUST_LOG is not set (e.g., "info", "debug")
///
/// # Returns
/// Ok(()) on success, or an error if tracing initialization fails
pub fn init_with_defaults(default_filter: &str) -> Result<()> {
    // Create the env filter with fallback
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    // Determine output format from environment
    let format = std::env::var("RUST_LOG_FORMAT").unwrap_or_else(|_| "pretty".to_string());

    // Build the subscriber based on format
    match format.as_str() {
        "json" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().with_ansi(false).json())
                .try_init()
                .map_err(|e| {
                    vm_core::error::VmError::Internal(format!(
                        "Failed to initialize tracing: {}",
                        e
                    ))
                })?;
        }
        "compact" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().compact())
                .try_init()
                .map_err(|e| {
                    vm_core::error::VmError::Internal(format!(
                        "Failed to initialize tracing: {}",
                        e
                    ))
                })?;
        }
        _ => {
            // Default to pretty format
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().pretty())
                .try_init()
                .map_err(|e| {
                    vm_core::error::VmError::Internal(format!(
                        "Failed to initialize tracing: {}",
                        e
                    ))
                })?;
        }
    }

    Ok(())
}

/// Initialize for testing with a specific configuration
#[cfg(test)]
pub fn init_for_testing() -> Result<()> {
    // Use a test-friendly configuration
    let env_filter = EnvFilter::new("debug");

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_test_writer())
        .try_init()
        .map_err(|e| {
            vm_core::error::VmError::Internal(format!("Failed to initialize test tracing: {}", e))
        })
}

/// Helper function to create a span with common fields
///
/// This is a convenience function for creating spans with standard fields.
/// Users should use tracing macros directly in most cases.
#[macro_export]
macro_rules! span_with_fields {
    ($level:expr, $name:expr, $($field:tt)*) => {
        tracing::span!($level, $name, $($field)*)
    };
}

/// Get the current span for adding fields
///
/// Example:
/// ```
/// use tracing::Span;
///
/// let span = Span::current();
/// span.record("user_id", &"user123");
/// ```
pub fn current_span() -> tracing::Span {
    tracing::Span::current()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::{debug, error, info, info_span, warn, Level};

    #[test]
    fn test_tracing_initialization() {
        // Note: Can only initialize once per process
        // This test may fail if run with other tests
        if init_for_testing().is_ok() {
            // Test that we can use tracing macros
            info!("Test info message");
            debug!("Test debug message");
            warn!("Test warning message");
            error!("Test error message");

            // Test span creation
            let span = info_span!("test_operation", id = 42);
            let _enter = span.enter();
            info!("Message within span");
        }
    }

    #[test]
    fn test_span_with_fields() {
        let span = span_with_fields!(Level::INFO, "test", user_id = "123", operation = "create");
        let _enter = span.enter();
        // Span is active for this scope
    }
}
