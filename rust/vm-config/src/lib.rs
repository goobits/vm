pub mod cli;
pub mod config;
mod config_ops;
mod embedded_presets;
pub mod merge;
mod paths; // Internal only
mod preset; // Internal only
pub mod resources; // VM resource suggestions
pub mod validate;
pub mod os_detection;
mod yaml_ops; // Internal only

// Re-export commonly needed path utilities
pub use paths::{get_tool_dir, resolve_tool_path, get_current_uid, get_current_gid};

// Re-export config operations for use by main vm binary
pub use config_ops::{load_global_config, ConfigOps};

// Re-export CLI functions for direct use
pub use cli::init_config_file;
