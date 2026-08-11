//! Package protocol server for npm, Cargo, PyPI, and managed tool artifacts.
//!
//! The server exposes native package-manager endpoints backed by immutable local
//! storage and read-through upstream caches. VM-specific workflow policy lives
//! in `vm-packages`; protocol modules stay focused on translating requests.

pub mod auth;
pub mod cargo;
pub mod config;
pub mod deletion;
pub mod error;
pub mod hash_utils;
pub mod local_storage;
pub mod npm;
pub mod package_utils;
pub mod pypi;
pub mod pypi_utils;
pub mod resolver;
pub mod server;
pub mod state;
pub mod storage;
mod tools;
pub mod upstream;
pub mod utils;
pub mod validation;
pub mod validation_utils;

pub use config::Config;
pub use error::{ApiErrorResponse, AppError, AppResult, ErrorCode};
pub use hash_utils::{sha1_hash, sha256_hash};
pub use pypi_utils::normalize_pypi_name;
pub use server::{run_server, run_server_background, run_server_with_shutdown};
pub use state::{AppState, SuccessResponse};
pub use upstream::{UpstreamClient, UpstreamConfig};
pub use validation::{
    escape_shell_arg, sanitize_docker_name, validate_file_size, validate_hostname,
    validate_package_name, validate_safe_path, validate_version, ValidationError, ValidationResult,
    MAX_DESCRIPTION_LENGTH, MAX_FILENAME_LENGTH, MAX_PACKAGE_NAME_LENGTH, MAX_PATH_DEPTH,
    MAX_UPLOAD_SIZE, MAX_VERSION_LENGTH,
};
pub use validation_utils::validate_filename;
