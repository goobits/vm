//! Enhanced output macros for the VM CLI.
//!
//! This module provides a set of macros for consistent, themed output
//! across all crates. It uses the `vm-messages` crate for templates
//! and formatting. All user-facing output is delegated to the `tracing`
//! crate to allow for structured logging.

/// A simple string formatting macro that replaces placeholders with values.
///
/// This macro takes a template string and a list of key-value pairs, and replaces
/// occurrences of `{key}` in the template with the corresponding value.
///
/// # Example
///
/// ```
/// let msg = simple_msg_format!("Hello, {name}!", name = "world");
/// assert_eq!(msg, "Hello, world!");
/// ```
#[macro_export]
macro_rules! simple_msg_format {
    ($template:expr) => {
        $template
    };
    ($template:expr, $($key:ident = $value:expr),+ $(,)?) => {
        {
            let mut result = $template.to_string();
            $(
                result = result.replace(&format!("{{{}}}", stringify!($key)), &$value.to_string());
            )+
            result
        }
    };
}

/// A macro for printing a formatted line to standard output, with a consistent style.
#[macro_export]
macro_rules! vm_println {
    () => {
        println!("");
    };
    ($($arg:tt)*) => {
        println!("{}", format!($($arg)*));
    }
}

/// A macro for printing a formatted error message to standard error.
#[macro_export]
macro_rules! vm_error {
    ($($arg:tt)*) => {
        eprintln!("{}", format!($($arg)*));
    }
}

/// A macro for logging the status of a VM operation.
///
/// This macro provides a consistent format for logging the start, success, and failure
/// of long-running operations.
#[macro_export]
macro_rules! vm_operation {
    (start $op:ident, name = $name:expr) => {
        $crate::vm_println!(
            "{}",
            simple_msg_format!(vm_messages::categories::VM_OPS.$op.starting, name = $name)
        );
    };
    (success $op:ident) => {
        $crate::vm_println!(
            "{}",
            simple_msg_format!(vm_messages::categories::VM_OPS.$op.success)
        );
    };
    (failed $op:ident, name = $name:expr, error = $error:expr) => {
        $crate::vm_error!(
            "{}",
            simple_msg_format!(vm_messages::categories::VM_OPS.$op.failed, name = $name)
        );
        $crate::vm_error!("   Error: {}", $error);
    };
}

/// A macro for providing suggestions to the user.
///
/// This macro is used to give helpful hints to the user when a command fails or
/// when they might be unsure of what to do next.
#[macro_export]
macro_rules! vm_suggest {
    (docker_check) => {
        $crate::vm_println!("💡 Try:\n  • Check Docker: docker ps\n  • Start Docker if stopped");
    };
    (vm_create) => {
        $crate::vm_println!("💡 Try:\n  • Create VM: vm create\n  • List VMs: vm list");
    };
    (custom $template:expr $(, $key:ident = $value:expr)*) => {
        $crate::vm_println!("{}", simple_msg_format!($template $(, $key = $value)*));
    };
}

/// A macro for displaying a hint along with an error message.
#[macro_export]
macro_rules! vm_error_hint {
    ($($arg:tt)*) => {
        tracing::info!("💡 {}", format!($($arg)*));
    };
}

/// A macro for logging an error message with additional details.
#[macro_export]
macro_rules! vm_error_with_details {
    ($main:expr, $details:expr) => {
        tracing::error!("❌ {}", $main);
        for detail in $details {
            tracing::error!("   └─ {}", detail);
        }
    };
}

/// A macro for logging a success message.
#[macro_export]
macro_rules! vm_success {
    ($($arg:tt)*) => {
        println!("✓ {}", format!($($arg)*));
    };
}

/// A macro for logging an informational message.
#[macro_export]
macro_rules! vm_info {
    ($($arg:tt)*) => {
        tracing::info!("ℹ {}", format!($($arg)*));
    };
}

/// A macro for logging a warning message.
#[macro_export]
macro_rules! vm_warning {
    ($($arg:tt)*) => {
        tracing::warn!("⚠ {}", format!($($arg)*));
    };
}

/// A macro for logging a progress message.
#[macro_export]
macro_rules! vm_progress {
    ($($arg:tt)*) => {
        tracing::info!("▶ {}", format!($($arg)*));
    };
}

/// A debug macro that is only enabled in debug builds.
///
/// This macro is a wrapper around `tracing::debug` that includes the file and line number
/// where it was called. It is compiled out in release builds.
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
