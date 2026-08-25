use bytes::Bytes;
use reqwest::Response;
use tracing::debug;

use crate::{validation, AppError, AppResult};

/// Read an upstream body into memory while enforcing the server-wide upload limit.
pub(crate) async fn read_bounded_response(
    response: Response,
    registry: &str,
    filename: &str,
) -> AppResult<Bytes> {
    if let Some(size) = response.content_length() {
        validation::validate_file_size(size, Some(validation::MAX_UPLOAD_SIZE))
            .map_err(|error| AppError::BadRequest(format!("{registry} file too large: {error}")))?;
    }

    let bytes = response.bytes().await.map_err(|error| {
        AppError::InternalError(format!("Failed to read {registry} file: {error}"))
    })?;
    validation::validate_file_size(bytes.len() as u64, Some(validation::MAX_UPLOAD_SIZE))
        .map_err(|error| AppError::BadRequest(format!("{registry} file too large: {error}")))?;

    debug!(
        operation = "read_upstream_body",
        filename,
        size = bytes.len(),
        registry,
        "upstream package body read"
    );
    Ok(bytes)
}
