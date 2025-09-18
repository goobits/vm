//! Demonstration of the enhanced structured logging system
//!
//! This example shows how to use the enhanced logging system with:
//! - Automatic context injection
//! - Module-scoped logging
//! - Environment variable configuration
//! - Tag-based filtering

use log::{debug, error, info, warn};
use vm_common::{
    log_context, module_logger::get_logger, module_logger_context,
    output_macros::init_structured_output, scoped_context, structured_log::LogConfig, vm_error,
    vm_progress, vm_success, vm_warning,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the structured logging system
    // This will read configuration from environment variables
    if let Err(e) = init_structured_output() {
        eprintln!("Failed to initialize logging: {}", e);
        return Err(format!("Logging initialization failed: {}", e).into());
    }

    println!("=== Enhanced Structured Logging Demo ===\n");

    // Demo 1: Basic context propagation
    demo_context_propagation();

    // Demo 2: Module-scoped logging
    demo_module_logging();

    // Demo 3: VM macros with context
    demo_vm_macros();

    // Demo 4: Environment variables
    demo_environment_config();

    Ok(())
}

fn demo_context_propagation() {
    println!("1. Context Propagation Demo");
    println!("---------------------------");

    // Create a scoped context that will be automatically included in all logs
    let _guard = scoped_context! {
        "request_id" => "req-12345",
        "user_id" => "user-789",
        "operation" => "vm_create"
    };

    info!("Starting VM creation process");

    // Nested context - inherits parent context
    {
        let _nested_guard = scoped_context! {
            "provider" => "docker",
            "image" => "ubuntu:22.04"
        };

        info!("Pulling container image");
        warn!("Image not found locally, downloading...");

        // Add more context to current layer
        log_context! {
            "download_size" => "847MB",
            "progress" => "45%"
        };

        info!("Download progress updated");
    }

    info!("VM creation completed");
    println!();
}

fn demo_module_logging() {
    println!("2. Module-Scoped Logging Demo");
    println!("------------------------------");

    // Get module-specific loggers
    let docker_logger = get_logger("vm_provider::docker");
    let vagrant_logger = get_logger("vm_provider::vagrant");

    // Use module context
    {
        let _guard = docker_logger.with_context();
        info!("Docker provider: Starting container");
        debug!("Docker provider: Container config validated");
    }

    {
        let _guard = vagrant_logger.with_context();
        info!("Vagrant provider: Starting VM");
        error!("Vagrant provider: VirtualBox not found");
    }

    // Using the module_logger_context macro
    module_logger_context!("vm_config::loader");
    info!("Configuration loaded successfully");

    println!();
}

fn demo_vm_macros() {
    println!("3. VM Output Macros Demo");
    println!("------------------------");

    let _guard = scoped_context! {
        "component" => "cli",
        "command" => "create"
    };

    vm_progress!("Initializing VM environment...");
    vm_progress!("Setting up networking...");
    vm_success!("VM successfully created!");
    vm_warning!("Port 8080 is already in use, using 8081 instead");
    vm_error!("Failed to mount shared directory");

    println!();
}

fn demo_environment_config() {
    println!("4. Environment Configuration Demo");
    println!("---------------------------------");

    // Show current configuration
    let config = LogConfig::from_env();
    println!("Current log level: {:?}", config.level);
    println!("Current log format: {:?}", config.format);
    println!("Current log output: {:?}", config.output);

    if let Some(ref tags) = config.tags {
        println!("Tag filters configured: {} patterns", tags.len());
        for tag in tags {
            println!("  - Key: {}, Value: {:?}", tag.key, tag.value);
        }
    } else {
        println!("No tag filters configured");
    }

    println!();
    println!("To test different configurations, try:");
    println!("  LOG_LEVEL=DEBUG cargo run --example enhanced_logging_demo");
    println!("  LOG_FORMAT=json cargo run --example enhanced_logging_demo");
    println!(
        "  LOG_TAGS=component:docker,operation:create* cargo run --example enhanced_logging_demo"
    );
    println!("  LOG_OUTPUT=file LOG_FILE=demo.log cargo run --example enhanced_logging_demo");
    println!();
}
