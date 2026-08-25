pub mod command_capture;
pub mod command_stream;
pub mod error;
pub mod file_system;
pub mod message;
pub mod output_macros;
pub mod prompts;
pub mod secrets;
pub mod system_check;
pub mod temp_dir;
pub mod user_paths;
pub mod validation;

/// Marker written beside source-built binaries so they can recover their
/// repository without keeping build artifacts in the source tree.
pub const SOURCE_WORKSPACE_MARKER: &str = ".vm-source-workspace";

/// Shared machine-local build directory used outside managed guest cache volumes.
pub const MACHINE_CARGO_TARGET_DIR: &str = "/tmp/vm-rust-target";

pub use system_check::check_system_resources;
