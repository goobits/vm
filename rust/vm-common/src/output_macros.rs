//! Enhanced output macros for the VM CLI.
//!
//! This module provides a set of macros for consistent, themed output
//! across all crates. It uses the `vm-messages` crate for templates
//! and formatting.

// Re-export the base msg macro for convenience
pub use crate::messages::msg;

#[macro_export]
macro_rules! vm_print {
    ($($arg:tt)*) => {
        print!("{}", format!($($arg)*));
    }
}

#[macro_export]
macro_rules! vm_println {
    () => {
        println!();
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
macro_rules! vm_operation {
    (start $op:ident, name = $name:expr) => {
        $crate::vm_println!(
            "{}",
            $crate::messages::msg!(
                $crate::messages::categories::VM_OPS.$op.starting,
                name = $name
            )
        );
    };
    (success $op:ident) => {
        $crate::vm_println!(
            "{}",
            $crate::messages::msg!($crate::messages::categories::VM_OPS.$op.success)
        );
    };
    (failed $op:ident, name = $name:expr, error = $error:expr) => {
        $crate::vm_error!(
            "{}",
            $crate::messages::msg!(
                $crate::messages::categories::VM_OPS.$op.failed,
                name = $name
            )
        );
        $crate::vm_error!("   Error: {}", $error);
    };
}

#[macro_export]
macro_rules! vm_suggest {
    (docker_check) => {
        $crate::vm_println!("💡 Try:\n  • Check Docker: docker ps\n  • Start Docker if stopped");
    };
    (vm_create) => {
        $crate::vm_println!("💡 Try:\n  • Create VM: vm create\n  • List VMs: vm list");
    };
    (custom $template:expr $(, $key:ident = $value:expr)*) => {
        $crate::vm_println!("{}", $crate::messages::msg!($template $(, $key = $value)*));
    };
}

#[macro_export]
macro_rules! vm_error_hint {
    ($($arg:tt)*) => {
        eprintln!("💡 {}", format!($($arg)*));
    };
}

#[macro_export]
macro_rules! vm_error_with_details {
    ($main:expr, $details:expr) => {
        eprintln!("❌ {}", $main);
        for detail in $details {
            eprintln!("   └─ {}", detail);
        }
    };
}

#[macro_export]
macro_rules! vm_success {
    ($($arg:tt)*) => {
        eprintln!("✓ {}", format!($($arg)*));
    };
}

#[macro_export]
macro_rules! vm_warning {
    ($($arg:tt)*) => {
        eprintln!("⚠ {}", format!($($arg)*));
    };
}

#[macro_export]
macro_rules! vm_progress {
    ($($arg:tt)*) => {
        eprintln!("▶ {}", format!($($arg)*));
    };
}

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

/// Initialize tracing for use with output macros
///
/// This function should be called early in application startup to enable
/// tracing features.
pub fn init_logging() -> anyhow::Result<()> {
    crate::tracing_init::init()
}

/// Initialize tracing with a default filter
pub fn init_logging_with_defaults(default_filter: &str) -> anyhow::Result<()> {
    crate::tracing_init::init_with_defaults(default_filter)
}
