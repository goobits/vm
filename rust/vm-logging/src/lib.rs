use std::{
    collections::HashMap,
    env,
    io::{self, IsTerminal, Write},
    path::Path,
};
use tracing::{field::Visit, span, subscriber::Interest, Metadata, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    fmt::MakeWriter,
    layer::{Context, Layer},
    prelude::*,
    registry, EnvFilter,
};

#[cfg(feature = "http")]
mod http;
#[cfg(feature = "http")]
pub use http::{request_context, HttpLogContext, REQUEST_ID_HEADER};

// --- Custom "Tee" Writer ---
struct Tee<A, B> {
    a: A,
    b: B,
}

impl<A, B> Write for Tee<A, B>
where
    A: Write,
    B: Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.a.write_all(buf)?;
        self.b.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.a.flush()?;
        self.b.flush()
    }
}

#[derive(Clone)]
struct MakeTee<A, B> {
    make_a: A,
    make_b: B,
}

impl<'a, A, B, W1, W2> MakeWriter<'a> for MakeTee<A, B>
where
    A: MakeWriter<'a, Writer = W1>,
    B: MakeWriter<'a, Writer = W2>,
    W1: Write + 'a,
    W2: Write + 'a,
{
    type Writer = Tee<W1, W2>;
    fn make_writer(&'a self) -> Self::Writer {
        Tee {
            a: self.make_a.make_writer(),
            b: self.make_b.make_writer(),
        }
    }
}

// --- Tag-Based Filtering Logic ---
#[derive(Clone, Debug)]
struct Tag {
    key: String,
    value: String,
}

struct TagFilterLayer {
    filters: Vec<Tag>,
}

impl<S> Layer<S> for TagFilterLayer
where
    S: Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        if self.filters.is_empty() || metadata.is_span() {
            Interest::always()
        } else {
            Interest::sometimes()
        }
    }

    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("Span should exist for just created ID");
        let mut fields = HashMap::new();
        let mut visitor = FieldVisitor(&mut fields);
        attrs.record(&mut visitor);
        span.extensions_mut().insert(fields);
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        let Some(fields) = extensions.get_mut::<HashMap<String, String>>() else {
            return;
        };
        values.record(&mut FieldVisitor(fields));
    }

    fn enabled(&self, meta: &Metadata<'_>, ctx: Context<'_, S>) -> bool {
        if self.filters.is_empty() {
            return true;
        }

        // Spans must be created before their fields can participate in tag
        // filtering. Events are filtered against the active span context.
        if meta.is_span() {
            return true;
        }

        let current_span = match ctx.lookup_current() {
            Some(span) => span,
            None => return false, // If tags are specified, events outside a span are filtered.
        };

        let mut all_fields = HashMap::new();
        for span_ref in current_span.scope() {
            if let Some(fields) = span_ref.extensions().get::<HashMap<String, String>>() {
                for (k, v) in fields {
                    all_fields.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
        }

        self.filters.iter().all(|filter| {
            all_fields
                .get(&filter.key)
                .is_some_and(|value| filter.value == "*" || value.contains(&filter.value))
        })
    }
}

struct FieldVisitor<'a>(&'a mut HashMap<String, String>);

impl Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LogOutput {
    Console,
    File,
    Both,
}

impl LogOutput {
    fn from_env(default: Self) -> Self {
        let value = env::var("LOG_OUTPUT").ok();
        Self::parse(value.as_deref(), default)
    }

    fn parse(value: Option<&str>, default: Self) -> Self {
        match value.unwrap_or_default().to_ascii_lowercase().as_str() {
            "console" => Self::Console,
            "file" => Self::File,
            "both" => Self::Both,
            _ => default,
        }
    }

    fn uses_console(self) -> bool {
        matches!(self, Self::Console | Self::Both)
    }

    fn uses_file(self) -> bool {
        matches!(self, Self::File | Self::Both)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LogFormat {
    Human,
    Json,
}

impl LogFormat {
    fn from_env(default: Self) -> Self {
        let value = env::var("LOG_FORMAT").ok();
        Self::parse(value.as_deref(), default)
    }

    fn parse(value: Option<&str>, default: Self) -> Self {
        match value.unwrap_or_default().to_ascii_lowercase().as_str() {
            "human" => Self::Human,
            "json" => Self::Json,
            "auto" => Self::automatic(),
            _ => default,
        }
    }

    fn automatic() -> Self {
        if io::stderr().is_terminal() {
            Self::Human
        } else {
            Self::Json
        }
    }
}

#[derive(Clone, Copy)]
struct LogDefaults {
    level: &'static str,
    output: LogOutput,
    format: LogFormat,
}

const CLI_DEFAULTS: LogDefaults = LogDefaults {
    level: "error",
    output: LogOutput::File,
    format: LogFormat::Human,
};

const SERVICE_DEFAULTS: LogDefaults = LogDefaults {
    level: "info",
    output: LogOutput::Console,
    format: LogFormat::Json,
};

/// Initializes the global tracing subscriber based on environment variables.
pub fn init_subscriber() -> Option<WorkerGuard> {
    init_with_defaults(CLI_DEFAULTS)
}

/// Initializes container-friendly structured logging for a long-running service.
pub fn init_service_subscriber() -> Option<WorkerGuard> {
    init_with_defaults(SERVICE_DEFAULTS)
}

fn init_with_defaults(defaults: LogDefaults) -> Option<WorkerGuard> {
    let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| defaults.level.to_string());
    let log_output = LogOutput::from_env(defaults.output);
    let log_format = LogFormat::from_env(defaults.format);
    let log_tags = env::var("LOG_TAGS").unwrap_or_else(|_| String::new());
    let log_file_path = env::var("LOG_FILE_PATH").unwrap_or_else(|_| "/tmp/vm.log".to_string());

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&log_level))
        .add_directive(
            "tokio=warn"
                .parse()
                .expect("hardcoded directive should be valid"),
        )
        .add_directive(
            "hyper=warn"
                .parse()
                .expect("hardcoded directive should be valid"),
        );

    let tag_filters = if log_tags.is_empty() {
        Vec::new()
    } else {
        log_tags
            .split(',')
            .filter_map(|s| {
                let mut parts = s.splitn(2, ':');
                let key = parts.next()?.trim().to_string();
                let value = parts.next()?.trim().to_string();
                Some(Tag { key, value })
            })
            .collect()
    };
    let tag_filter_layer = TagFilterLayer {
        filters: tag_filters,
    };

    let use_console = log_output.uses_console();
    let use_file = log_output.uses_file();
    let is_json = log_format == LogFormat::Json;

    let mut guard: Option<WorkerGuard> = None;

    let subscriber = registry().with(env_filter).with(tag_filter_layer);

    let log_path = Path::new(&log_file_path);
    let log_dir = log_path.parent().unwrap_or_else(|| Path::new("/tmp"));
    let log_filename = log_path.file_name().unwrap_or("vm.log".as_ref());

    if use_console && use_file {
        let file_appender = tracing_appender::rolling::daily(log_dir, log_filename);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        guard = Some(_guard);

        let tee_writer = MakeTee {
            make_a: std::io::stderr,
            make_b: non_blocking,
        };

        let fmt_layer = tracing_subscriber::fmt::layer().with_writer(tee_writer);
        if is_json {
            subscriber.with(fmt_layer.json()).init();
        } else {
            subscriber.with(fmt_layer.pretty()).init();
        }
    } else if use_console {
        let fmt_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
        if is_json {
            subscriber.with(fmt_layer.json()).init();
        } else {
            subscriber.with(fmt_layer.pretty()).init();
        }
    } else if use_file {
        let file_appender = tracing_appender::rolling::daily(log_dir, log_filename);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        guard = Some(_guard);

        let fmt_layer = tracing_subscriber::fmt::layer().with_writer(non_blocking);
        if is_json {
            subscriber.with(fmt_layer.json()).init();
        } else {
            subscriber.with(fmt_layer.pretty()).init();
        }
    } else {
        subscriber.init();
    }

    guard
}

#[cfg(test)]
mod tests {
    use super::{LogFormat, LogOutput, Tag, TagFilterLayer, Tee};
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::{fmt::MakeWriter, prelude::*};

    struct FailingWriter;

    #[derive(Clone)]
    struct BufferWriter(Arc<Mutex<Vec<u8>>>);

    struct BufferGuard(Arc<Mutex<Vec<u8>>>);

    impl Write for BufferGuard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| io::Error::other("log buffer lock poisoned"))?
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for BufferWriter {
        type Writer = BufferGuard;

        fn make_writer(&'a self) -> Self::Writer {
            BufferGuard(Arc::clone(&self.0))
        }
    }

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("sink unavailable"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn tee_writes_the_complete_event_to_both_sinks() {
        let mut tee = Tee {
            a: Vec::new(),
            b: Vec::new(),
        };

        tee.write_all(b"event").unwrap();

        assert_eq!(tee.a, b"event");
        assert_eq!(tee.b, b"event");
    }

    #[test]
    fn tee_reports_a_failed_sink() {
        let mut tee = Tee {
            a: Vec::new(),
            b: FailingWriter,
        };

        assert!(tee.write_all(b"event").is_err());
    }

    #[test]
    fn output_modes_report_their_sinks() {
        assert!(LogOutput::Console.uses_console());
        assert!(!LogOutput::Console.uses_file());
        assert!(!LogOutput::File.uses_console());
        assert!(LogOutput::File.uses_file());
        assert!(LogOutput::Both.uses_console());
        assert!(LogOutput::Both.uses_file());
    }

    #[test]
    fn output_parser_is_case_insensitive_and_falls_back_safely() {
        assert_eq!(
            LogOutput::parse(Some("CONSOLE"), LogOutput::File),
            LogOutput::Console
        );
        assert_eq!(
            LogOutput::parse(Some("invalid"), LogOutput::File),
            LogOutput::File
        );
    }

    #[test]
    fn format_parser_is_case_insensitive_and_falls_back_safely() {
        assert_eq!(
            LogFormat::parse(Some("JSON"), LogFormat::Human),
            LogFormat::Json
        );
        assert_eq!(
            LogFormat::parse(Some("invalid"), LogFormat::Human),
            LogFormat::Human
        );
    }

    #[test]
    fn tag_filter_keeps_matching_root_and_recorded_span_events() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry()
            .with(TagFilterLayer {
                filters: vec![Tag {
                    key: "request_id".into(),
                    value: "keep".into(),
                }],
            })
            .with(
                tracing_subscriber::fmt::layer()
                    .without_time()
                    .with_ansi(false)
                    .with_writer(BufferWriter(Arc::clone(&output))),
            );

        tracing::subscriber::with_default(subscriber, || {
            let matching = tracing::info_span!("request", request_id = "keep-1");
            matching.in_scope(|| tracing::info!("included event"));

            let recorded = tracing::info_span!("request", request_id = tracing::field::Empty);
            recorded.record("request_id", "keep-2");
            recorded.in_scope(|| tracing::info!("recorded event"));

            let excluded = tracing::info_span!("request", request_id = "drop-1");
            excluded.in_scope(|| tracing::info!("excluded event"));
        });

        let bytes = output.lock().unwrap().clone();
        let logs = String::from_utf8(bytes).unwrap();
        assert!(logs.contains("included event"), "captured logs: {logs:?}");
        assert!(logs.contains("recorded event"), "captured logs: {logs:?}");
        assert!(!logs.contains("excluded event"), "captured logs: {logs:?}");
    }
}
