use std::{
    collections::HashMap,
    env,
    io::{self, Write},
    path::Path,
};
use tracing::{field::Visit, span, Metadata, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    fmt::MakeWriter,
    layer::{Context, Layer},
    prelude::*,
    registry, EnvFilter,
};

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
        let res_a = self.a.write(buf);
        let res_b = self.b.write(buf);
        res_a.or(res_b)
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
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut fields = HashMap::new();
        let mut visitor = FieldVisitor(&mut fields);
        attrs.record(&mut visitor);
        span.extensions_mut().insert(fields);
    }

    fn enabled(&self, _meta: &Metadata<'_>, ctx: Context<'_, S>) -> bool {
        if self.filters.is_empty() {
            return true;
        }

        let scope = match ctx.current_span().id().and_then(|id| ctx.span_scope(id)) {
            Some(scope) => scope,
            None => return false, // If tags are specified, events outside a span are filtered.
        };

        let mut all_fields = HashMap::new();
        for span_ref in scope {
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

/// Initializes the global tracing subscriber based on environment variables.
pub fn init_subscriber() -> Option<WorkerGuard> {
    let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let log_output = env::var("LOG_OUTPUT").unwrap_or_else(|_| "console".to_string());
    let log_format = env::var("LOG_FORMAT").unwrap_or_else(|_| "human".to_string());
    let log_tags = env::var("LOG_TAGS").unwrap_or_else(|_| String::new());
    let log_file_path = env::var("LOG_FILE_PATH").unwrap_or_else(|_| "/tmp/vm.log".to_string());

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&log_level))
        .add_directive("tokio=warn".parse().unwrap())
        .add_directive("hyper=warn".parse().unwrap());

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

    let use_console = log_output == "console" || log_output == "both";
    let use_file = log_output == "file" || log_output == "both";
    let is_json = log_format == "json";

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
            make_a: std::io::stdout,
            make_b: non_blocking,
        };

        let fmt_layer = tracing_subscriber::fmt::layer().with_writer(tee_writer);
        if is_json {
            subscriber.with(fmt_layer.json()).init();
        } else {
            subscriber.with(fmt_layer.pretty()).init();
        }
    } else if use_console {
        let fmt_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);
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
