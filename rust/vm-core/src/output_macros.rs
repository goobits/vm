//! Shared terminal output primitives.
//!
//! Requested data is written to stdout. Progress, warnings, hints, and errors
//! are written to stderr so commands can be composed safely.

#[macro_export]
macro_rules! vm_println {
    () => {
        println!("");
    };
    ($($arg:tt)*) => {
        println!("{}", format!($($arg)*));
    }
}

#[macro_export]
macro_rules! vm_error {
    ($($arg:tt)*) => {
        eprintln!("{}", format!($($arg)*));
    }
}

#[macro_export]
macro_rules! vm_error_details {
    ($main:expr, $details:expr) => {
        eprintln!("Error: {}", $main);
        for detail in $details {
            eprintln!("  {}", detail);
        }
    };
}

#[macro_export]
macro_rules! vm_hint {
    ($($arg:tt)*) => {
        eprintln!("Hint: {}", format!($($arg)*));
    };
}

#[macro_export]
macro_rules! vm_success {
    ($($arg:tt)*) => {
        println!("✓ {}", format!($($arg)*));
    };
}

#[macro_export]
macro_rules! vm_info {
    ($($arg:tt)*) => {
        println!("{}", format!($($arg)*));
    };
}

#[macro_export]
macro_rules! vm_warning {
    ($($arg:tt)*) => {
        eprintln!("Warning: {}", format!($($arg)*));
    };
}

#[macro_export]
macro_rules! vm_progress {
    ($($arg:tt)*) => {
        eprintln!("{}", format!($($arg)*));
    };
}

#[macro_export]
macro_rules! vm_dbg {
    () => {
        #[cfg(debug_assertions)]
        {
            tracing::debug!("[{}:{}]", file!(), line!());
        }
    };
    ($val:expr $(,)?) => {{
        #[cfg(debug_assertions)]
        {
            match $val {
                tmp => {
                    tracing::debug!("[{}:{}] {} = {:#?}",
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

// Note: Output macros for consistent CLI formatting across crates
