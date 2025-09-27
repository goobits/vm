//! Tests for the tracing initialization module

use tracing::{debug, error, info, info_span, warn};

#[test]
fn test_basic_tracing() {
    // Note: Tracing can only be initialized once per process
    // so we can't test initialization itself in unit tests.
    // This test just verifies that tracing macros work.

    info!("Test info message");
    warn!("Test warning message");
    error!("Test error message");
    debug!("Test debug message");
}

#[test]
fn test_tracing_spans() {
    // Test span creation and entering
    let span = info_span!("test_operation", id = 42, name = "test");
    let _enter = span.enter();

    info!("Message within span");

    // Test nested spans
    {
        let inner_span = info_span!("inner_operation");
        let _inner_enter = inner_span.enter();
        info!("Message within inner span");
    }
}

#[test]
fn test_structured_fields() {
    // Test structured logging with fields
    let user_id = "user123";
    let operation = "create_vm";
    let duration_ms = 1500;

    info!(
        user_id = %user_id,
        operation = %operation,
        duration_ms = duration_ms,
        "Operation completed"
    );
}

#[test]
fn test_span_with_fields_macro() {
    use tracing::Level;
    use vm_common::span_with_fields;

    let span = span_with_fields!(Level::INFO, "test_span", user = "alice", action = "login");
    let _enter = span.enter();

    info!("User action recorded");
}
