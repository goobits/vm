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

pub fn init() {
    static LOGGER: VmLogger = VmLogger;
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(LevelFilter::Info);
}
