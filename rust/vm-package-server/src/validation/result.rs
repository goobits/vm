//! # Validation Result Type

use crate::validation::error::ValidationError;

/// Result type for validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;