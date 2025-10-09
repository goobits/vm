use tracing::Level;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initializes the tracing subscriber.
///
/// This function sets up the global tracing subscriber for the application.
/// It supports two modes, controlled by the `VM_JSON_LOGS` environment variable:
///
/// 1.  **JSON format**: If `VM_JSON_LOGS` is set to `1` or `true`, all logs will be
///     emitted as structured JSON to stderr. This is ideal for consumption by log
///     aggregation services in CI/CD or production environments.
///
/// 2.  **Human-readable format**: By default, or if `VM_JSON_LOGS` is not set
///     to a truthy value, logs are split:
///     - `INFO` level logs are sent to `stdout` without any extra formatting to mimic `println!`.
///     - `WARN`, `ERROR`, `DEBUG`, `TRACE` logs are sent to `stderr` with standard formatting.
///     This is suitable for local development to preserve the CLI's look and feel.
///
/// Log verbosity is controlled by the `RUST_LOG` environment variable.
/// If not set, it defaults to `info`.
///
/// # Panics
///
/// This function will panic if it fails to initialize the subscriber, as logging
/// is considered a critical component of the application.
pub fn init_subscriber() {
    let use_json = std::env::var("VM_JSON_LOGS")
        .map(|val| val == "1" || val.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    if use_json {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json())
            .init();
    } else {
        let info_layer = fmt::layer()
            .with_writer(std::io::stdout)
            .with_level(false)
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .without_time()
            .with_ansi(false)
            .with_span_events(fmt::format::FmtSpan::NONE)
            .with_filter(tracing_subscriber::filter::filter_fn(|meta| {
                meta.level() == &Level::INFO
            }));

        let other_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_filter(tracing_subscriber::filter::filter_fn(|meta| {
                meta.level() != &Level::INFO
            }));

        tracing_subscriber::registry()
            .with(env_filter)
            .with(info_layer)
            .with(other_layer)
            .init();
    }
}