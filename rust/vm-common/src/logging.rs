// Re-export the structured logging system for backward compatibility
pub use crate::structured_log::{init, init_with_config, LogConfig, LogFormat, LogOutput};

// External crates
use log::{Level, LevelFilter, Metadata, Record};

#[deprecated(
    since = "1.3.0",
    note = "VmLogger is deprecated. Use the structured logging system instead. This will be removed in v2.0.0"
)]
pub struct VmLogger;

impl log::Log for VmLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let level_str = match record.level() {
                Level::Error => "❌ Error",
                Level::Warn => "⚠️ Warning",
                Level::Info => "✅",
                Level::Debug => "🐛",
                Level::Trace => "🔬",
            };
            println!("{} {}: {}", level_str, record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Legacy init function - now initializes structured logging
/// This maintains backward compatibility for any existing code
#[deprecated(
    since = "1.3.0",
    note = "Use `structured_log::init()` or `structured_log::init_with_config()` instead. This legacy interface will be removed in v2.0.0"
)]
pub fn init_legacy() {
    static LOGGER: VmLogger = VmLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(LevelFilter::Info);
}
