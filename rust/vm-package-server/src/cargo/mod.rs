//! Cargo registry implementation
//!
//! This module provides Cargo package registry functionality including
//! index management, package uploads, downloads, and metadata operations.

use serde_json::Value;
use vm_packages::{PackageEcosystem, PackageIdentity};

use crate::{AppError, AppResult};

mod handlers;
mod index;
mod parsing;
mod storage;

#[cfg(test)]
mod tests;

/// Metadata extracted from a crate upload
#[derive(Debug)]
pub struct CrateMetadata {
    pub name: String,
    pub version: String,
    pub deps: Value,
    pub features: Value,
}

pub use handlers::*;
pub use index::*;

fn validate_crate_name(name: &str) -> AppResult<&str> {
    PackageIdentity::new(PackageEcosystem::Cargo, name)
        .map_err(|error| AppError::BadRequest(format!("Invalid crate name '{name}': {error}")))?;
    Ok(name)
}
