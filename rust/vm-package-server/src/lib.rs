//! Package protocol server for npm, Cargo, PyPI, and managed tool artifacts.
//!
//! The server exposes native package-manager endpoints backed by immutable local
//! storage and read-through upstream caches. VM-specific workflow policy lives
//! in `vm-packages`; protocol modules stay focused on translating requests.

mod auth;
mod cargo;
mod config;
mod error;
mod hash_utils;
mod internal;
mod local_storage;
mod npm;
mod package_utils;
mod pypi;
mod pypi_utils;
mod resolver;
mod response_body;
mod server;
mod state;
mod storage;
mod tools;
mod upstream;
mod utils;
mod validation;

pub use config::Config;
pub use error::{ApiErrorResponse, AppError, AppResult, ErrorCode};
pub use hash_utils::{sha1_hash, sha256_hash};
pub use internal::InternalRegistryClient;
pub use pypi_utils::normalize_pypi_name;
pub use resolver::ResolverService;
pub use server::{run_server, run_server_with_shutdown};
pub use state::{AppState, SuccessResponse};
pub use upstream::{UpstreamClient, UpstreamConfig};
pub use validation::{
    escape_shell_arg, sanitize_docker_name, validate_base64_characters, validate_base64_size,
    validate_cargo_upload_structure, validate_docker_image_name, validate_docker_port,
    validate_docker_volume_path, validate_file_size, validate_filename, validate_hostname,
    validate_multipart_limits, validate_package_name, validate_package_upload, validate_safe_path,
    validate_total_upload_size, validate_version, ValidationError, ValidationResult,
    MAX_BASE64_DECODED_SIZE, MAX_BASE64_ENCODED_SIZE, MAX_DESCRIPTION_LENGTH, MAX_FILENAME_LENGTH,
    MAX_METADATA_SIZE, MAX_MULTIPART_FIELDS, MAX_PACKAGE_FILE_SIZE, MAX_PACKAGE_NAME_LENGTH,
    MAX_PATH_DEPTH, MAX_REQUEST_BODY_SIZE, MAX_UPLOAD_SIZE, MAX_VERSION_LENGTH, MEMORY_THRESHOLD,
};
