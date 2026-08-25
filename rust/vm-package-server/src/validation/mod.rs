//! # Input Validation Utilities
//!
//! This module provides security-focused validation helpers for sanitizing and validating
//! various types of input data throughout the package server. All functions follow a
//! security-first approach to prevent injection attacks and ensure data integrity.
//!
//! ## Security Features
//!
//! - Path traversal attack prevention with strict validation
//! - Size limits and bounds checking to prevent resource exhaustion
//!
pub mod error;
pub mod http;
pub mod limits;
pub mod manifests;
pub mod paths;
pub mod result;

pub use self::{
    http::{validate_base64_characters, validate_base64_size, validate_multipart_limits},
    limits::{
        validate_file_size, validate_package_upload, validate_total_upload_size, MAX_METADATA_SIZE,
        MAX_MULTIPART_FIELDS, MAX_REQUEST_BODY_SIZE, MAX_UPLOAD_SIZE,
    },
    manifests::{validate_cargo_upload_structure, validate_package_name, validate_version},
    paths::{validate_filename, validate_safe_path},
};
