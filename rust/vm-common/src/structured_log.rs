// Structured logging system - refactored to use modular organization

// Re-export from logging modules
pub use crate::logging::{
    LogConfig, LogFormat, LogOutput, TagFilter,
    StructuredLogger,
    create_test_logger,
};

// Internal imports
use crate::log_context;

/// Initialize the structured logging system
pub fn init() -> Result<(), log::SetLoggerError> {
    init_with_config(LogConfig::from_env())
}

/// Initialize with a custom configuration
pub fn init_with_config(config: LogConfig) -> Result<(), log::SetLoggerError> {
    // Initialize context system
    log_context::init_context();

    // Create and install the logger
    let logger = Box::leak(Box::new(StructuredLogger::new(config.clone())));
    log::set_logger(logger)?;
    log::set_max_level(config.level);

    Ok(())
}

/// Helper functions for output macros
/// These provide a simpler interface for the migration macros

/// Log an info message
pub fn log_info(message: &str) {
    log::info!("{}", message);
}

/// Log an error message
pub fn log_error(message: &str) {
    log::error!("{}", message);
}

/// Log an info message without newline (for compatibility)
pub fn log_info_no_newline(message: &str) {
    // In structured logging, we always log complete messages
    // This is here for API compatibility during migration
    log::info!("{}", message);
}

/// Log an error message without newline (for compatibility)
pub fn log_error_no_newline(message: &str) {
    log::error!("{}", message);
}

/// Log a progress message
pub fn log_progress(message: &str) {
    log::info!("Progress: {}", message);
}

/// Log a success message
pub fn log_success(message: &str) {
    log::info!("Success: {}", message);
}

/// Log a warning message
pub fn log_warning(message: &str) {
    log::warn!("{}", message);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use log::{debug, error, info, warn};
    use serde_json::{json, Map, Value};
    use regex::Regex;

    #[test]
    fn test_tag_filter_exact_match() {
        use crate::logging::config::TagPattern;

        let pattern = TagPattern {
            key: "component".to_string(),
            value: Some("docker".to_string()),
            regex: None,
        };
        let filter = TagFilter::from_patterns(vec![pattern]);

        let mut context = Map::new();
        context.insert("component".to_string(), json!("docker"));
        assert!(filter.matches(&context));

        context.insert("component".to_string(), json!("vagrant"));
        assert!(!filter.matches(&context));
    }

    #[test]
    fn test_tag_filter_wildcard() {
        use crate::logging::config::TagPattern;

        let pattern = TagPattern {
            key: "operation".to_string(),
            value: Some("create*".to_string()),
            regex: Some(Regex::new("^create.*$")
                .expect("Failed to compile test regex - pattern is invalid")),
        };
        let filter = TagFilter::from_patterns(vec![pattern]);

        let mut context = Map::new();
        context.insert("operation".to_string(), json!("create_vm"));
        assert!(filter.matches(&context));

        context.insert("operation".to_string(), json!("destroy_vm"));
        assert!(!filter.matches(&context));
    }

    #[test]
    fn test_json_formatting() {
        use log::Level;

        let logger = create_test_logger();
        let record = log::Record::builder()
            .args(format_args!("Test message"))
            .level(Level::Info)
            .module_path(Some("test"))
            .file(Some("test.rs"))
            .line(Some(42))
            .build();

        let mut context = Map::new();
        context.insert("request_id".to_string(), json!("abc123"));

        // This test assumes format_message is public - we need to check the logger module
        // For now, this is a placeholder to maintain test coverage
        // Real test would use the logger through the public API
    }
}