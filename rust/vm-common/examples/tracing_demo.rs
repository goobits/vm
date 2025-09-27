//! Example demonstrating the new tracing-based logging system

use anyhow::Result;
use tracing::{debug, error, info, info_span, warn};
use vm_common::tracing_init;

fn main() -> Result<()> {
    // Initialize tracing with a debug level
    std::env::set_var("RUST_LOG", "debug");
    tracing_init::init()?;

    info!("Starting tracing demo");

    // Demonstrate basic logging
    demo_basic_logging();

    // Demonstrate spans
    demo_spans();

    // Demonstrate structured logging
    demo_structured_logging();

    info!("Tracing demo completed");
    Ok(())
}

fn demo_basic_logging() {
    let span = info_span!("basic_logging_demo");
    let _enter = span.enter();

    info!("This is an info message");
    warn!("This is a warning message");
    error!("This is an error message");
    debug!("This is a debug message");
}

fn demo_spans() {
    let span = info_span!("spans_demo", operation = "test");
    let _enter = span.enter();

    info!("Starting operation");

    // Nested span
    {
        let inner_span = info_span!("inner_operation", step = 1);
        let _inner = inner_span.enter();

        info!("Processing step 1");
        debug!("Step 1 details");
    }

    {
        let inner_span = info_span!("inner_operation", step = 2);
        let _inner = inner_span.enter();

        info!("Processing step 2");
        debug!("Step 2 details");
    }

    info!("Operation completed");
}

fn demo_structured_logging() {
    let span = info_span!("structured_demo");
    let _enter = span.enter();

    // Log with structured fields
    let user_id = "user123";
    let vm_name = "dev-vm";
    let provider = "docker";

    info!(
        user_id = %user_id,
        vm_name = %vm_name,
        provider = %provider,
        "Creating VM"
    );

    // Simulate some work
    std::thread::sleep(std::time::Duration::from_millis(100));

    let duration_ms = 100;
    info!(
        user_id = %user_id,
        vm_name = %vm_name,
        duration_ms = duration_ms,
        "VM created successfully"
    );

    // Error with context
    error!(
        user_id = %user_id,
        vm_name = "prod-vm",
        error = "Connection timeout",
        "Failed to create VM"
    );
}
