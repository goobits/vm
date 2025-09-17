//! Output macros for gradual migration to structured logging
//!
//! These macros provide a migration path from direct println!/eprintln! usage
//! to structured logging. They maintain backward compatibility while allowing
//! gradual adoption of structured logging throughout the codebase.
//!
//! All macros automatically inject the current logging context when structured
//! logging is enabled, providing rich contextual information without requiring
//! code changes at call sites.
/// Print a line to stdout, with optional structured logging
///
/// When structured logging is enabled, automatically includes current context.
/// Routes to appropriate log level (INFO) with proper output routing.
#[macro_export]
macro_rules! vm_println {
    () => {
        println!();
    };
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            let _guard = logger.with_context();
            log::info!("{}", format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            println!($($arg)*);
        }
    }};
}

/// Print a line to stderr, with optional structured logging
///
/// When structured logging is enabled, automatically includes current context.
/// Routes to appropriate log level (ERROR) with proper output routing.
#[macro_export]
macro_rules! vm_eprintln {
    () => {
        eprintln!();
    };
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            let _guard = logger.with_context();
            log::error!("{}", format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!($($arg)*);
        }
    }};
}

/// Print without newline to stdout
///
/// Note: In structured logging mode, this still produces complete log entries
/// as structured logs are discrete events, not streaming output.
#[macro_export]
macro_rules! vm_print {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            let _guard = logger.with_context();
            log::info!("{}", format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            print!($($arg)*);
        }
    }};
}

/// Print without newline to stderr
///
/// Note: In structured logging mode, this still produces complete log entries
/// as structured logs are discrete events, not streaming output.
#[macro_export]
macro_rules! vm_eprint {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            let _guard = logger.with_context();
            log::error!("{}", format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprint!($($arg)*);
        }
    }};
}

/// Debug print (only in debug builds)
///
/// In structured logging mode, uses DEBUG level with rich context.
/// Only active in debug builds for performance.
#[macro_export]
macro_rules! vm_dbg {
    () => {
        #[cfg(debug_assertions)]
        {
            #[cfg(feature = "structured-output")]
            {
                let module_name = module_path!();
                let logger = $crate::module_logger::get_logger(module_name);
                let _guard = logger.with_context();
                $crate::log_context! {
                    "message_type" => "debug",
                    "file" => file!(),
                    "line" => line!()
                };
                log::debug!("Debug checkpoint");
            }
            #[cfg(not(feature = "structured-output"))]
            {
                eprintln!("[{}:{}]", file!(), line!());
            }
        }
    };
    ($val:expr $(,)?) => {{
        #[cfg(debug_assertions)]
        {
            #[cfg(feature = "structured-output")]
            {
                match $val {
                    tmp => {
                        let module_name = module_path!();
                        let logger = $crate::module_logger::get_logger(module_name);
                        let _guard = logger.with_context();
                        $crate::log_context! {
                            "message_type" => "debug",
                            "file" => file!(),
                            "line" => line!(),
                            "variable" => stringify!($val)
                        };
                        log::debug!("{} = {:#?}", stringify!($val), &tmp);
                        tmp
                    }
                }
            }
            #[cfg(not(feature = "structured-output"))]
            {
                match $val {
                    tmp => {
                        eprintln!("[{}:{}] {} = {:#?}",
                            file!(), line!(), stringify!($val), &tmp);
                        tmp
                    }
                }
            }
        }
        #[cfg(not(debug_assertions))]
        {
            $val
        }
    }};
    ($($val:expr),+ $(,)?) => {
        ($($crate::vm_dbg!($val)),+,)
    };
}

/// Progress indicator print (for CLI progress messages)
///
/// Uses INFO level with progress context when structured logging is enabled.
#[macro_export]
macro_rules! vm_progress {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            let _guard = logger.with_context();
            $crate::log_context! {
                "message_type" => "progress"
            };
            log::info!("{}", format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!("▶ {}", format!($($arg)*));
        }
    }};
}

/// Success message print
///
/// Uses INFO level with success context when structured logging is enabled.
#[macro_export]
macro_rules! vm_success {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            let _guard = logger.with_context();
            $crate::log_context! {
                "message_type" => "success",
                "status" => "success"
            };
            log::info!("{}", format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!("✓ {}", format!($($arg)*));
        }
    }};
}

/// Warning message print
///
/// Uses WARN level with proper context when structured logging is enabled.
#[macro_export]
macro_rules! vm_warning {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            let _guard = logger.with_context();
            $crate::log_context! {
                "message_type" => "warning",
                "status" => "warning"
            };
            log::warn!("{}", format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!("⚠ {}", format!($($arg)*));
        }
    }};
}

/// Error message print
///
/// Uses ERROR level with proper context when structured logging is enabled.
#[macro_export]
macro_rules! vm_error {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            let module_name = module_path!();
            let logger = $crate::module_logger::get_logger(module_name);
            let _guard = logger.with_context();
            $crate::log_context! {
                "message_type" => "error",
                "status" => "error"
            };
            log::error!("{}", format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!("❌ {}", format!($($arg)*));
        }
    }};
}

/// Initialize structured logging for use with output macros
///
/// This function should be called early in application startup to enable
/// structured logging features. If not called, macros will fall back to
/// standard print macros.
pub fn init_structured_output() -> Result<(), log::SetLoggerError> {
    crate::structured_log::init()
}

/// Initialize structured logging with custom configuration
pub fn init_structured_output_with_config(
    config: crate::structured_log::LogConfig,
) -> Result<(), log::SetLoggerError> {
    crate::structured_log::init_with_config(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_println() {
        // Just ensure macros compile
        vm_println!("Test message");
        vm_println!("Test with {}", "argument");
        vm_println!();
    }

    #[test]
    fn test_vm_eprintln() {
        vm_eprintln!("Error message");
        vm_eprintln!("Error with {}", "argument");
        vm_eprintln!();
    }

    #[test]
    fn test_specialized_macros() {
        vm_progress!("Processing...");
        vm_success!("Operation completed");
        vm_warning!("Something to note");
        vm_error!("Something went wrong");
    }

    #[test]
    fn test_vm_dbg() {
        let value = 42;
        let result = vm_dbg!(value);
        assert_eq!(result, 42);

        let (a, b) = vm_dbg!(1 + 1, 2 * 2);
        assert_eq!(a, 2);
        assert_eq!(b, 4);
    }

    #[test]
    fn test_init_functions() {
        // Test that initialization functions don't panic
        // Note: Multiple init calls are safe but will only use the first one
        let _ = init_structured_output();

        use crate::structured_log::{LogConfig, LogFormat, LogOutput};
        let config = LogConfig {
            level: log::Level::Debug,
            format: LogFormat::Json,
            output: LogOutput::Console,
            tags: None,
        };
        let _ = init_structured_output_with_config(config);
    }
}
