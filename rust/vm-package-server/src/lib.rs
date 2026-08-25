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
pub use resolver::ResolverService;
pub use server::{run_server, run_server_with_shutdown};
pub use state::{AppState, SuccessResponse};
pub use upstream::{UpstreamClient, UpstreamConfig};
