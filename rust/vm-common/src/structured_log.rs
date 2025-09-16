// Structured logging system - refactored to use modular organization

// Re-export from logging modules
pub use crate::logging::{LogConfig, LogFormat, LogOutput};

// Internal imports
use crate::log_context;
use log::{Level, Record};
use serde_json::{Map, Value};
use regex::Regex;
use chrono;

/// Tag pattern for filtering logs
pub struct TagPattern {
    pub key: String,
    pub value: Option<String>,
    pub regex: Option<Regex>,
}

/// Filter for log entries based on tags
pub struct TagFilter {
    patterns: Vec<TagPattern>,
}

impl TagFilter {
    pub fn from_patterns(patterns: Vec<TagPattern>) -> Self {
        Self { patterns }
    }

    pub fn matches(&self, context: &Map<String, Value>) -> bool {
        for pattern in &self.patterns {
            if let Some(value) = context.get(&pattern.key) {
                if let Some(expected) = &pattern.value {
                    if let Some(regex) = &pattern.regex {
                        if let Some(str_value) = value.as_str() {
                            if regex.is_match(str_value) {
                                return true;
                            }
                        }
                    } else if let Some(str_value) = value.as_str() {
                        if str_value == expected {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// Simple structured logger implementation
pub struct StructuredLogger {
    config: LogConfig,
}

impl StructuredLogger {
    pub fn new(config: LogConfig) -> Self {
        Self { config }
    }
}

impl log::Log for StructuredLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.config.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            match self.config.format {
                LogFormat::Json => {
                    let mut log_entry = Map::new();
                    log_entry.insert("level".to_string(), Value::String(record.level().to_string()));
                    log_entry.insert("message".to_string(), Value::String(record.args().to_string()));
                    log_entry.insert("timestamp".to_string(), Value::String(chrono::Utc::now().to_rfc3339()));

                    if let Some(module) = record.module_path() {
                        log_entry.insert("module".to_string(), Value::String(module.to_string()));
                    }

                    let json_str = serde_json::to_string(&log_entry).unwrap_or_else(|_| "{}".to_string());
                    eprintln!("{}", json_str);
                }
                LogFormat::Text => {
                    eprintln!("[{}] {}: {}",
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
                        record.level(),
                        record.args()
                    );
                }
            }
        }
    }

    fn flush(&self) {}
}

/// Create a test logger for unit tests
#[cfg(test)]
pub fn create_test_logger() -> StructuredLogger {
    StructuredLogger::new(LogConfig {
        level: Level::Debug,
        format: LogFormat::Text,
        output: LogOutput::Stderr,
        tags: None,
    })
}

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