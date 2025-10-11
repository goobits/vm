pub mod command_stream;
pub mod error;
pub mod file_system;
pub mod output_macros;
pub mod platform;
pub mod project;
pub mod system_check;
pub mod temp_dir;
pub mod user_paths;

// Re-export system resource detection functions for convenience
pub use system_check::{check_system_resources, get_cpu_core_count, get_total_memory_gb};
