pub mod config;
pub mod merge;
pub mod validate;
pub mod cli;
mod preset;          // Internal only
mod yaml_ops;
mod paths;           // Internal only
mod embedded_presets;
mod config_ops;      // Internal only

// Re-export commonly needed path utilities
pub use paths::resolve_tool_path;
