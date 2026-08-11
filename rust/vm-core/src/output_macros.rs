//! Shared terminal output primitives.
//!
//! Requested data is written to stdout. Progress, warnings, hints, and errors
//! are written to stderr so commands can be composed safely.

use std::fmt;
use std::io::{self, Write};

fn write_output(mut writer: impl Write, arguments: fmt::Arguments<'_>, newline: bool) {
    let result = writer.write_fmt(arguments).and_then(|()| {
        if newline {
            writer.write_all(b"\n")?;
        }
        writer.flush()
    });

    // A downstream command may close its pipe after receiving enough data.
    // Terminal output is best-effort, so that must never turn a successful VM
    // operation into a panic.
    let _ = result;
}

#[doc(hidden)]
pub fn write_stdout(arguments: fmt::Arguments<'_>, newline: bool) {
    write_output(io::stdout().lock(), arguments, newline);
}

#[doc(hidden)]
pub fn write_stderr(arguments: fmt::Arguments<'_>, newline: bool) {
    write_output(io::stderr().lock(), arguments, newline);
}

#[macro_export]
macro_rules! vm_print {
    ($($arg:tt)*) => {{
        $crate::output_macros::write_stdout(format_args!($($arg)*), false);
    }}
}

#[macro_export]
macro_rules! vm_println {
    () => {{
        $crate::output_macros::write_stdout(format_args!(""), true);
    }};
    ($($arg:tt)*) => {{
        $crate::output_macros::write_stdout(format_args!($($arg)*), true);
    }}
}

#[macro_export]
macro_rules! vm_error {
    ($($arg:tt)*) => {{
        $crate::output_macros::write_stderr(format_args!($($arg)*), true);
    }}
}

#[macro_export]
macro_rules! vm_hint {
    ($($arg:tt)*) => {{
        $crate::output_macros::write_stderr(
            format_args!("Hint: {}", format_args!($($arg)*)),
            true,
        );
    }}
}

#[macro_export]
macro_rules! vm_success {
    ($($arg:tt)*) => {{
        $crate::output_macros::write_stdout(
            format_args!("✓ {}", format_args!($($arg)*)),
            true,
        );
    }}
}

#[macro_export]
macro_rules! vm_info {
    ($($arg:tt)*) => {{
        $crate::output_macros::write_stdout(format_args!($($arg)*), true);
    }}
}

#[macro_export]
macro_rules! vm_warning {
    ($($arg:tt)*) => {{
        $crate::output_macros::write_stderr(
            format_args!("Warning: {}", format_args!($($arg)*)),
            true,
        );
    }}
}

#[macro_export]
macro_rules! vm_progress {
    ($($arg:tt)*) => {{
        $crate::output_macros::write_stderr(format_args!($($arg)*), true);
    }}
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
