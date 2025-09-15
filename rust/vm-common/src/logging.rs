// Re-export the structured logging system for backward compatibility
pub use crate::structured_log::{init, init_with_config, LogConfig, LogFormat, LogOutput};

// External crates
use log::{Level, LevelFilter, Metadata, Record};

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
pub fn init_legacy() {
    static LOGGER: VmLogger = VmLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(LevelFilter::Info);
}
