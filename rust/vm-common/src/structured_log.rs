// Standard library
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// External crates
use chrono::Utc;
use is_terminal::IsTerminal;
use log::{Level, LevelFilter, Metadata, Record};
use regex::Regex;
use serde_json::{json, Map, Value};

// Internal imports
use crate::log_context;

/// Configuration for the structured logger
#[derive(Debug, Clone)]
pub struct LogConfig {
    pub level: LevelFilter,
    pub output: LogOutput,
    pub format: LogFormat,
    pub tags: TagFilter,
    pub file_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum LogOutput {
    Console,
    File,
    Both,
}

#[derive(Debug, Clone)]
pub enum LogFormat {
    Json,
    Human,
    Auto,
}

#[derive(Debug, Clone)]
pub struct TagFilter {
    patterns: Vec<TagPattern>,
    show_all: bool,
}

#[derive(Debug, Clone)]
struct TagPattern {
    key: String,
    value: Option<String>,
    regex: Option<Regex>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: LevelFilter::Info,
            output: LogOutput::Console,
            format: LogFormat::Auto,
            tags: TagFilter::show_all(),
            file_path: None,
        }
    }
}

impl LogConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Self {
        let level = parse_log_level();
        let output = parse_log_output();
        let format = parse_log_format();
        let tags = parse_log_tags();
        let file_path = determine_file_path(&output);

        Self {
            level,
            output,
            format,
            tags,
            file_path,
        }
    }
}

impl TagFilter {
    fn show_all() -> Self {
        Self {
            patterns: vec![],
            show_all: true,
        }
    }

    fn from_patterns(patterns: Vec<TagPattern>) -> Self {
        Self {
            patterns,
            show_all: false,
        }
    }

    fn matches(&self, context: &Map<String, Value>) -> bool {
        if self.show_all {
            return true;
        }

        self.patterns.iter().any(|pattern| {
            if let Some(context_value) = context.get(&pattern.key) {
                match &pattern.value {
                    Some(expected) => {
                        if let Some(regex) = &pattern.regex {
                            regex.is_match(&context_value.to_string())
                        } else {
                            context_value.as_str() == Some(expected)
                        }
                    }
                    None => true, // Key exists, value doesn't matter
                }
            } else {
                false
            }
        })
    }
}

/// The main structured logger
pub struct StructuredLogger {
    config: LogConfig,
    file_handle: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
}

impl StructuredLogger {
    pub fn new(config: LogConfig) -> Self {
        let file_handle = if matches!(config.output, LogOutput::File | LogOutput::Both) {
            config.file_path.as_ref().and_then(|path| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .ok()
                    .map(|f| Arc::new(Mutex::new(Box::new(f) as Box<dyn Write + Send>)))
            })
        } else {
            None
        };

        Self {
            config,
            file_handle,
        }
    }

    fn format_message(&self, record: &Record, context: &Map<String, Value>) -> (String, String) {
        let format = match &self.config.format {
            LogFormat::Auto => {
                if is_tty() {
                    LogFormat::Human
                } else {
                    LogFormat::Json
                }
            }
            format => format.clone(),
        };

        match format {
            LogFormat::Json => {
                let json_msg = self.format_json(record, context);
                (json_msg.clone(), json_msg)
            }
            LogFormat::Human => {
                let human_msg = self.format_human(record, context);
                (human_msg.clone(), human_msg)
            }
            LogFormat::Auto => unreachable!(),
        }
    }

    fn format_json(&self, record: &Record, context: &Map<String, Value>) -> String {
        let mut log_entry = json!({
            "timestamp": Utc::now().to_rfc3339(),
            "level": record.level().to_string(),
            "message": record.args().to_string(),
            "module": record.module_path().unwrap_or("unknown"),
            "file": record.file().unwrap_or("unknown"),
            "line": record.line().unwrap_or(0)
        });

        // Merge in context
        if let Some(obj) = log_entry.as_object_mut() {
            for (key, value) in context {
                obj.insert(key.clone(), value.clone());
            }
        }

        serde_json::to_string(&log_entry)
            .unwrap_or_else(|_| "Failed to serialize log entry".to_string())
    }

    fn format_human(&self, record: &Record, context: &Map<String, Value>) -> String {
        let level_icon = match record.level() {
            Level::Error => "❌",
            Level::Warn => "⚠️ ",
            Level::Info => "✅",
            Level::Debug => "🐛",
            Level::Trace => "🔬",
        };

        let timestamp = Utc::now().format("%H:%M:%S");
        let message = record.args().to_string();

        // Build context string
        let context_str = if context.is_empty() {
            String::new()
        } else {
            let pairs: Vec<String> = context
                .iter()
                .map(|(k, v)| format!("{}={}", k, format_value_for_human(v)))
                .collect();
            format!(" ({})", pairs.join(", "))
        };

        format!("{} [{}] {}{}", level_icon, timestamp, message, context_str)
    }

    fn should_output_to_stdout(&self, level: Level) -> bool {
        matches!(level, Level::Info | Level::Debug | Level::Trace)
    }

    fn write_to_console(&self, message: &str, level: Level) {
        if matches!(self.config.output, LogOutput::Console | LogOutput::Both) {
            if self.should_output_to_stdout(level) {
                let _ = io::stdout().write_all(message.as_bytes());
                let _ = io::stdout().write_all(b"\n");
                let _ = io::stdout().flush();
            } else {
                let _ = io::stderr().write_all(message.as_bytes());
                let _ = io::stderr().write_all(b"\n");
                let _ = io::stderr().flush();
            }
        }
    }

    fn write_to_file(&self, message: &str) {
        if let Some(handle) = &self.file_handle {
            if let Ok(mut writer) = handle.lock() {
                let _ = writer.write_all(message.as_bytes());
                let _ = writer.write_all(b"\n");
                let _ = writer.flush();
            }
        }
    }
}

impl log::Log for StructuredLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.config.level.to_level().unwrap_or(Level::Info)
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        // Get current context
        let context = log_context::current_context();

        // Check if this log should be shown based on tag filtering
        if !self.config.tags.matches(&context) {
            return;
        }

        // Format the message
        let (console_msg, file_msg) = self.format_message(record, &context);

        // Output to console
        self.write_to_console(&console_msg, record.level());

        // Output to file
        if matches!(self.config.output, LogOutput::File | LogOutput::Both) {
            self.write_to_file(&file_msg);
        }
    }

    fn flush(&self) {
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();

        if let Some(handle) = &self.file_handle {
            if let Ok(mut writer) = handle.lock() {
                let _ = writer.flush();
            }
        }
    }
}

// Environment variable parsing functions

fn parse_log_level() -> LevelFilter {
    env::var("LOG_LEVEL")
        .ok()
        .and_then(|s| match s.to_uppercase().as_str() {
            "ERROR" => Some(LevelFilter::Error),
            "WARN" => Some(LevelFilter::Warn),
            "INFO" => Some(LevelFilter::Info),
            "DEBUG" => Some(LevelFilter::Debug),
            "TRACE" => Some(LevelFilter::Trace),
            "OFF" => Some(LevelFilter::Off),
            _ => None,
        })
        .unwrap_or(LevelFilter::Info)
}

fn parse_log_output() -> LogOutput {
    env::var("LOG_OUTPUT")
        .ok()
        .and_then(|s| match s.to_lowercase().as_str() {
            "console" => Some(LogOutput::Console),
            "file" => Some(LogOutput::File),
            "both" => Some(LogOutput::Both),
            _ => None,
        })
        .unwrap_or(LogOutput::Console)
}

fn parse_log_format() -> LogFormat {
    env::var("LOG_FORMAT")
        .ok()
        .and_then(|s| match s.to_lowercase().as_str() {
            "json" => Some(LogFormat::Json),
            "human" => Some(LogFormat::Human),
            "auto" => Some(LogFormat::Auto),
            _ => None,
        })
        .unwrap_or(LogFormat::Auto)
}

fn parse_log_tags() -> TagFilter {
    let tags_str = match env::var("LOG_TAGS") {
        Ok(s) if s.trim() == "*" => return TagFilter::show_all(),
        Ok(s) if s.trim().is_empty() => return TagFilter::show_all(),
        Ok(s) => s,
        Err(_) => return TagFilter::show_all(),
    };

    let mut patterns = Vec::new();
    for tag in tags_str.split(',') {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }

        if let Some((key, value)) = tag.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim();

            if value == "*" {
                // Key exists, any value
                patterns.push(TagPattern {
                    key,
                    value: None,
                    regex: None,
                });
            } else if value.contains('*') || value.contains('?') {
                // Wildcard pattern
                let regex_pattern = value.replace('*', ".*").replace('?', ".");
                if let Ok(regex) = Regex::new(&format!("^{}$", regex_pattern)) {
                    patterns.push(TagPattern {
                        key,
                        value: Some(value.to_string()),
                        regex: Some(regex),
                    });
                }
            } else {
                // Exact match
                patterns.push(TagPattern {
                    key,
                    value: Some(value.to_string()),
                    regex: None,
                });
            }
        }
    }

    if patterns.is_empty() {
        TagFilter::show_all()
    } else {
        TagFilter::from_patterns(patterns)
    }
}

fn determine_file_path(output: &LogOutput) -> Option<PathBuf> {
    if matches!(output, LogOutput::File | LogOutput::Both) {
        // Check if we're in a container (no file logging in containers by default)
        if is_container() {
            return None;
        }

        // Use LOG_FILE env var if set, otherwise default
        env::var("LOG_FILE")
            .ok()
            .map(PathBuf::from)
            .or_else(|| dirs::cache_dir().map(|dir| dir.join("vm").join("vm.log")))
    } else {
        None
    }
}

fn is_tty() -> bool {
    io::stdout().is_terminal()
}

fn is_container() -> bool {
    // Check common container indicators
    Path::new("/.dockerenv").exists()
        || env::var("KUBERNETES_SERVICE_HOST").is_ok()
        || env::var("container").is_ok()
}

fn format_value_for_human(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => value.to_string(),
    }
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

/// Create a logger instance for testing
#[cfg(test)]
pub fn create_test_logger() -> StructuredLogger {
    StructuredLogger::new(LogConfig {
        level: LevelFilter::Debug,
        output: LogOutput::Console,
        format: LogFormat::Human,
        tags: TagFilter::show_all(),
        file_path: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use log::{debug, error, info, warn};

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
            regex: Some(Regex::new("^create.*$").unwrap()),
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

        let (_, msg) = logger.format_message(&record, &context);
        let parsed: Value = serde_json::from_str(&msg).expect("Valid JSON");

        assert_eq!(parsed["level"], "INFO");
        assert_eq!(parsed["message"], "Test message");
        assert_eq!(parsed["request_id"], "abc123");
    }
}
