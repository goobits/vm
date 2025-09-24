//! Module-scoped logging functionality
//!
//! This module provides module-specific logger instances that automatically
//! inject module context into all log messages. Each module gets its own
//! logger instance with pre-configured context.
//!
//! ## Usage
//!
//! ```rust
//! use vm_common::module_logger::get_logger;
//!
//! // Get a logger for this module
//! let logger = get_logger("vm_provider::docker");
//!
//! // Use standard log macros - module context is automatically added
//! log::info!("Starting container creation");
//! let error = "connection timeout";
//! log::error!("Failed to create container: {}", error);
//! ```
//!
//! ## Features
//!
//! - Automatic module name injection into log context
//! - Per-module log level control via environment variables
//! - Seamless integration with existing log macros
//! - Module hierarchy support (e.g., "vm_provider::docker::lifecycle")

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter};
use serde_json::{Map, Value};

use crate::log_context::{scoped_context, ContextGuard};

/// Global registry of module-specific loggers
static MODULE_LOGGERS: OnceLock<Mutex<HashMap<String, ModuleLogger>>> = OnceLock::new();

/// A module-specific logger that automatically injects module context
#[derive(Debug, Clone)]
pub struct ModuleLogger {
    module_name: String,
    level_filter: LevelFilter,
}

impl ModuleLogger {
    /// Create a new module logger
    fn new(module_name: String) -> Self {
        let level_filter = Self::parse_module_level(&module_name);
        Self {
            module_name,
            level_filter,
        }
    }

    /// Parse module-specific log level from environment variables
    ///
    /// Supports hierarchical module level configuration:
    /// - LOG_LEVEL_vm_provider=DEBUG
    /// - LOG_LEVEL_vm_provider__docker=TRACE  (note: double underscore for ::)
    /// - LOG_LEVEL_vm_provider__docker__lifecycle=ERROR
    fn parse_module_level(module_name: &str) -> LevelFilter {
        // Try exact match first
        let env_key = format!("LOG_LEVEL_{}", module_name.replace("::", "__"));
        if let Ok(level_str) = std::env::var(&env_key) {
            return Self::parse_level_filter(&level_str);
        }

        // Try parent module levels (hierarchical fallback)
        let parts: Vec<&str> = module_name.split("::").collect();
        for i in (1..parts.len()).rev() {
            let parent_module = parts[..i].join("::");
            let parent_env_key = format!("LOG_LEVEL_{}", parent_module.replace("::", "__"));
            if let Ok(level_str) = std::env::var(&parent_env_key) {
                return Self::parse_level_filter(&level_str);
            }
        }

        // Fallback to global LOG_LEVEL
        if let Ok(level_str) = std::env::var("LOG_LEVEL") {
            return Self::parse_level_filter(&level_str);
        }

        // Default to Warn
        LevelFilter::Warn
    }

    /// Parse a level string into LevelFilter
    fn parse_level_filter(level_str: &str) -> LevelFilter {
        match level_str.to_uppercase().as_str() {
            "OFF" => LevelFilter::Off,
            "ERROR" => LevelFilter::Error,
            "WARN" => LevelFilter::Warn,
            "INFO" => LevelFilter::Info,
            "DEBUG" => LevelFilter::Debug,
            "TRACE" => LevelFilter::Trace,
            _ => LevelFilter::Warn,
        }
    }

    /// Check if logging is enabled for the given level
    pub fn enabled(&self, level: Level) -> bool {
        level <= self.level_filter
    }

    /// Create a context guard with module information
    ///
    /// This automatically adds the module name to the logging context
    /// for the duration of the guard's lifetime.
    pub fn with_context(&self) -> ContextGuard {
        let mut context = Map::new();
        context.insert(
            "module".to_string(),
            Value::String(self.module_name.clone()),
        );

        // Extract component hierarchy for better filtering
        let parts: Vec<&str> = self.module_name.split("::").collect();
        if let Some(component) = parts.first() {
            context.insert(
                "component".to_string(),
                Value::String(component.to_string()),
            );
        }
        if parts.len() > 1 {
            if let Some(subcomponent) = parts.get(1) {
                context.insert(
                    "subcomponent".to_string(),
                    Value::String(subcomponent.to_string()),
                );
            }
        }

        scoped_context(context)
    }

    /// Get the module name
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// Get the current level filter
    pub fn level_filter(&self) -> LevelFilter {
        self.level_filter
    }
}

/// Get a logger for the specified module
///
/// This function returns a cached logger instance for the module,
/// creating it if it doesn't exist. The logger automatically injects
/// module context into all log messages.
///
/// ## Arguments
///
/// * `module_name` - The name of the module (e.g., "vm_provider::docker")
///
/// ## Returns
///
/// A `ModuleLogger` instance configured for the specified module.
///
/// ## Example
///
/// ```rust
/// use vm_common::module_logger::get_logger;
/// use log::info;
///
/// let _logger = get_logger("vm_provider::docker");
/// let _guard = _logger.with_context();
///
/// // This log will automatically include module context
/// info!("Container operation started");
/// ```
pub fn get_logger(module_name: &str) -> ModuleLogger {
    let registry = MODULE_LOGGERS.get_or_init(|| Mutex::new(HashMap::new()));

    {
        let loggers = registry.lock().unwrap_or_else(|poisoned| {
            // If the mutex is poisoned, recover the data and continue
            eprintln!("Warning: Module logger registry mutex was poisoned, recovering...");
            poisoned.into_inner()
        });
        if let Some(logger) = loggers.get(module_name) {
            return logger.clone();
        }
    }

    // Create new logger if not found
    let logger = ModuleLogger::new(module_name.to_string());

    {
        let mut loggers = registry.lock().unwrap_or_else(|poisoned| {
            // If the mutex is poisoned, recover the data and continue
            eprintln!(
                "Warning: Module logger registry mutex was poisoned during insert, recovering..."
            );
            poisoned.into_inner()
        });
        loggers.insert(module_name.to_string(), logger.clone());
    }

    logger
}

/// Macro for getting a module logger and setting up context in one call
///
/// This macro automatically derives the module name from the current module path
/// and creates a context guard that lasts for the current scope.
///
/// ## Usage
///
/// ```rust
/// use vm_common::module_logger_context;
/// use log::info;
///
/// fn my_function() {
///     module_logger_context!();
///
///     // All logs in this scope will have module context
///     info!("Function started");
///
///     // ... function body ...
///
///     info!("Function completed");
/// } // Context is automatically cleaned up here
/// ```
#[macro_export]
macro_rules! module_logger_context {
    () => {
        let _module_logger_guard = {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            logger.with_context()
        };
    };
    ($module:expr) => {
        let _module_logger_guard = {
            let logger = $crate::module_logger::get_logger($module);
            logger.with_context()
        };
    };
}

/// Convenience macro for module-scoped logging
///
/// This macro combines getting a module logger and logging in one call.
/// It automatically sets up the module context for the log message.
///
/// ## Usage
///
/// ```rust
/// use vm_common::module_log;
///
/// // These will automatically include module context
/// module_log!(info, "Operation started");
/// let error_message = "connection timeout";
/// module_log!(error, "Operation failed: {}", error_message);
/// let value = 42;
/// module_log!(debug, "Debug info: value={}", value);
/// ```
#[macro_export]
macro_rules! module_log {
    ($level:ident, $($arg:tt)*) => {
        {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            let _guard = logger.with_context();
            log::$level!($($arg)*);
        }
    };
}

/// List all registered module loggers (useful for debugging)
pub fn list_module_loggers() -> Vec<String> {
    let registry = MODULE_LOGGERS.get_or_init(|| Mutex::new(HashMap::new()));
    let loggers = registry.lock().unwrap_or_else(|poisoned| {
        // If the mutex is poisoned, recover the data and continue
        eprintln!("Warning: Module logger registry mutex was poisoned during list, recovering...");
        poisoned.into_inner()
    });
    loggers.keys().cloned().collect()
}

/// Clear all module logger cache (mainly for testing)
#[cfg(any(test, feature = "test-helpers"))]
pub fn clear_module_loggers() {
    let registry = MODULE_LOGGERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut loggers = registry.lock().unwrap_or_else(|poisoned| {
        // If the mutex is poisoned, recover the data and continue
        eprintln!("Warning: Module logger registry mutex was poisoned during clear, recovering...");
        poisoned.into_inner()
    });
    loggers.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    // Test synchronization to prevent race conditions with global logger state
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_get_logger() {
        let _guard = TEST_MUTEX.lock().unwrap();
        clear_module_loggers();

        let logger1 = get_logger("test_module");
        let logger2 = get_logger("test_module");

        // Should return the same instance (cached)
        assert_eq!(logger1.module_name(), logger2.module_name());

        let logger3 = get_logger("different_module");
        assert_ne!(logger1.module_name(), logger3.module_name());
    }

    #[test]
    fn test_module_level_parsing() {
        let _guard = TEST_MUTEX.lock().unwrap();
        clear_module_loggers();
        // Test exact match
        env::set_var("LOG_LEVEL_test__module", "DEBUG");
        let logger = get_logger("test::module");
        assert_eq!(logger.level_filter(), LevelFilter::Debug);
        env::remove_var("LOG_LEVEL_test__module");

        // Test parent module fallback
        env::set_var("LOG_LEVEL_vm_provider", "WARN");
        let logger = get_logger("vm_provider::docker::lifecycle");
        assert_eq!(logger.level_filter(), LevelFilter::Warn);
        env::remove_var("LOG_LEVEL_vm_provider");

        // Test global fallback
        env::set_var("LOG_LEVEL", "ERROR");
        let logger = get_logger("unknown::module");
        assert_eq!(logger.level_filter(), LevelFilter::Error);
        env::remove_var("LOG_LEVEL");
    }

    #[test]
    fn test_level_filter_parsing() {
        assert_eq!(
            ModuleLogger::parse_level_filter("DEBUG"),
            LevelFilter::Debug
        );
        assert_eq!(
            ModuleLogger::parse_level_filter("debug"),
            LevelFilter::Debug
        );
        assert_eq!(ModuleLogger::parse_level_filter("INFO"), LevelFilter::Info);
        assert_eq!(
            ModuleLogger::parse_level_filter("invalid"),
            LevelFilter::Warn
        );
    }

    #[test]
    fn test_logger_enabled() {
        let _guard = TEST_MUTEX.lock().unwrap();
        clear_module_loggers();
        env::set_var("LOG_LEVEL_test_enabled", "WARN");
        let logger = get_logger("test_enabled");

        assert!(!logger.enabled(Level::Debug));
        assert!(!logger.enabled(Level::Info));
        assert!(logger.enabled(Level::Warn));
        assert!(logger.enabled(Level::Error));

        env::remove_var("LOG_LEVEL_test_enabled");
    }

    #[test]
    fn test_with_context() {
        let _guard = TEST_MUTEX.lock().unwrap();
        clear_module_loggers();
        use crate::log_context;

        log_context::clear_context();

        let logger = get_logger("vm_provider::docker");
        let _guard = logger.with_context();

        let context = log_context::current_context();
        assert_eq!(
            context.get("module").unwrap().as_str(),
            Some("vm_provider::docker")
        );
        assert_eq!(
            context.get("component").unwrap().as_str(),
            Some("vm_provider")
        );
        assert_eq!(
            context.get("subcomponent").unwrap().as_str(),
            Some("docker")
        );
    }

    #[test]
    fn test_list_module_loggers() {
        let _guard = TEST_MUTEX.lock().unwrap();

        clear_module_loggers();

        // Create unique test loggers
        let _logger1 = get_logger("test_list_module_loggers_mod1");
        let _logger2 = get_logger("test_list_module_loggers_mod2");

        let loggers = list_module_loggers();

        // Check that our specific test loggers are present
        assert!(loggers.contains(&"test_list_module_loggers_mod1".to_string()));
        assert!(loggers.contains(&"test_list_module_loggers_mod2".to_string()));

        // Verify we have exactly our 2 test loggers
        assert_eq!(loggers.len(), 2);
    }
}
