/// Output macros for gradual migration to structured logging
///
/// These macros provide a migration path from direct println!/eprintln! usage
/// to structured logging. They maintain backward compatibility while allowing
/// gradual adoption of structured logging throughout the codebase.

/// Print a line to stdout, with optional structured logging
#[macro_export]
macro_rules! vm_println {
    () => {
        println!();
    };
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            use $crate::structured_log;
            structured_log::log_info(&format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            println!($($arg)*);
        }
    }};
}

/// Print a line to stderr, with optional structured logging
#[macro_export]
macro_rules! vm_eprintln {
    () => {
        eprintln!();
    };
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            use $crate::structured_log;
            structured_log::log_error(&format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!($($arg)*);
        }
    }};
}

/// Print without newline to stdout
#[macro_export]
macro_rules! vm_print {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            use $crate::structured_log;
            structured_log::log_info_no_newline(&format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            print!($($arg)*);
        }
    }};
}

/// Print without newline to stderr
#[macro_export]
macro_rules! vm_eprint {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            use $crate::structured_log;
            structured_log::log_error_no_newline(&format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprint!($($arg)*);
        }
    }};
}

/// Debug print (only in debug builds)
#[macro_export]
macro_rules! vm_dbg {
    () => {
        #[cfg(debug_assertions)]
        {
            eprintln!("[{}:{}]", file!(), line!());
        }
    };
    ($val:expr $(,)?) => {{
        #[cfg(debug_assertions)]
        {
            match $val {
                tmp => {
                    eprintln!("[{}:{}] {} = {:#?}",
                        file!(), line!(), stringify!($val), &tmp);
                    tmp
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
#[macro_export]
macro_rules! vm_progress {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            use $crate::structured_log;
            structured_log::log_progress(&format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!("▶ {}", format!($($arg)*));
        }
    }};
}

/// Success message print
#[macro_export]
macro_rules! vm_success {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            use $crate::structured_log;
            structured_log::log_success(&format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!("✓ {}", format!($($arg)*));
        }
    }};
}

/// Warning message print
#[macro_export]
macro_rules! vm_warning {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            use $crate::structured_log;
            structured_log::log_warning(&format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!("⚠ {}", format!($($arg)*));
        }
    }};
}

/// Error message print
#[macro_export]
macro_rules! vm_error {
    ($($arg:tt)*) => {{
        #[cfg(feature = "structured-output")]
        {
            use $crate::structured_log;
            structured_log::log_error(&format!($($arg)*));
        }
        #[cfg(not(feature = "structured-output"))]
        {
            eprintln!("✗ {}", format!($($arg)*));
        }
    }};
}

#[cfg(test)]
mod tests {
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
}