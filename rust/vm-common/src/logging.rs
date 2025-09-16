// Re-export the structured logging system for backward compatibility
pub use crate::structured_log::{init, init_with_config, LogConfig, LogFormat, LogOutput};

// External crates
use log::{Level, LevelFilter, Metadata, Record};

// Deprecated code removed in preparation for v2.0.0
// All logging functionality has been migrated to the structured logging system
// Use `structured_log::init()` or `structured_log::init_with_config()` instead
