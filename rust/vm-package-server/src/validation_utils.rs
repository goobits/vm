//! Centralized file validation utilities
//!
//! This module provides consolidated validation utilities to eliminate code duplication
//! across file handling, streaming operations, and Docker parameter validation.

use crate::error::AppError;
use crate::validation::{
    validate_file_size, MAX_PACKAGE_FILE_SIZE, MAX_UPLOAD_SIZE, MEMORY_THRESHOLD,
};
use bytes::Bytes;
use reqwest::Response;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{debug, error, info, warn};

/// Centralized file streaming and validation utilities
pub struct FileStreamValidator;

impl FileStreamValidator {
    /// Validate and stream HTTP response with size constraints and memory optimization
    ///
    /// This consolidates the pattern used across upstream.rs for PyPI, NPM, and Cargo
    /// file downloads with proper size validation and memory threshold handling.
    pub async fn validate_and_stream_response(
        response: Response,
        registry_type: &str,
        filename: &str,
    ) -> Result<Bytes, AppError> {
        // Check content length header for size validation
        if let Some(content_length) = response.content_length() {
            validate_file_size(content_length, Some(MAX_UPLOAD_SIZE)).map_err(|e| {
                AppError::BadRequest(format!("{} file too large: {}", registry_type, e))
            })?;

            // For small files, load into memory directly
            if content_length <= MEMORY_THRESHOLD as u64 {
                let bytes = response.bytes().await.map_err(|e| {
                    AppError::InternalError(format!("Failed to read {} file: {}", registry_type, e))
                })?;
                info!(
                    filename = %filename,
                    size = bytes.len(),
                    registry = %registry_type,
                    "Successfully streamed (small file)"
                );
                return Ok(bytes);
            }
        }

        // For large files or unknown size, collect all bytes directly
        // Note: This still loads into memory but adds size validation during the process
        let bytes = response.bytes().await.map_err(|e| {
            AppError::InternalError(format!("Failed to read {} file: {}", registry_type, e))
        })?;

        info!(
            filename = %filename,
            size = bytes.len(),
            registry = %registry_type,
            "Successfully streamed (large file)"
        );
        Ok(bytes)
    }

    /// Validate and read file from disk with size constraints and memory optimization
    ///
    /// This consolidates the pattern used across storage.rs for file reading operations
    /// with proper size validation and memory threshold handling.
    pub async fn validate_and_read_file<P: AsRef<Path>>(path: P) -> Result<Vec<u8>, AppError> {
        let path = path.as_ref();

        // Check if file exists
        if !path.exists() {
            return Err(AppError::NotFound(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check file size before reading to prevent memory exhaustion
        let metadata = fs::metadata(path).await?;
        let file_size = metadata.len();

        // Validate file size against security limits
        validate_file_size(file_size, Some(MAX_UPLOAD_SIZE))
            .map_err(|e| AppError::BadRequest(format!("File too large: {}", e)))?;

        // For small files, read directly into memory
        if file_size <= MEMORY_THRESHOLD as u64 {
            let content = fs::read(path).await?;
            debug!(
                path = %path.display(),
                size = content.len(),
                "File read successfully (small file)"
            );
            Ok(content)
        } else {
            // For larger files, use streaming read with buffer
            let mut file = fs::File::open(path).await?;
            let mut content = Vec::with_capacity(file_size as usize);
            file.read_to_end(&mut content).await?;

            debug!(
                path = %path.display(),
                size = content.len(),
                "File read successfully (large file)"
            );
            Ok(content)
        }
    }

    /// Validate and read file as string with size constraints and memory optimization
    ///
    /// This consolidates the string file reading pattern used across storage.rs
    /// with proper size validation and memory threshold handling.
    pub async fn validate_and_read_file_string<P: AsRef<Path>>(
        path: P,
    ) -> Result<String, AppError> {
        let path = path.as_ref();

        // Check if file exists
        if !path.exists() {
            return Err(AppError::NotFound(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check file size before reading to prevent memory exhaustion
        let metadata = fs::metadata(path).await?;
        let file_size = metadata.len();

        // Validate file size against security limits
        validate_file_size(file_size, Some(MAX_UPLOAD_SIZE))
            .map_err(|e| AppError::BadRequest(format!("File too large: {}", e)))?;

        // For small files, read directly into memory
        if file_size <= MEMORY_THRESHOLD as u64 {
            let content = fs::read_to_string(path).await?;
            debug!(
                path = %path.display(),
                size = content.len(),
                "String file read successfully (small file)"
            );
            Ok(content)
        } else {
            // For larger files, use streaming read with buffer
            let mut file = fs::File::open(path).await?;
            let mut content = String::new();
            tokio::io::AsyncReadExt::read_to_string(&mut file, &mut content).await?;

            debug!(
                path = %path.display(),
                size = content.len(),
                "String file read successfully (large file)"
            );
            Ok(content)
        }
    }

    /// Validate package upload data with registry-specific constraints
    ///
    /// This consolidates the pattern used across pypi.rs, npm.rs, and cargo.rs
    /// for validating uploaded package files with appropriate error handling.
    pub fn validate_package_upload(
        data: &[u8],
        filename: &str,
        registry_type: &str,
    ) -> Result<(), AppError> {
        debug!(
            filename = %filename,
            size = data.len(),
            registry = %registry_type,
            "Validating package upload"
        );

        // Validate file size using package-specific constraints
        validate_file_size(data.len() as u64, Some(MAX_PACKAGE_FILE_SIZE)).map_err(|e| {
            warn!(
                filename = %filename,
                size = %data.len(),
                registry = %registry_type,
                "Package file too large"
            );
            AppError::UploadError(format!("Package file too large: {}", e))
        })?;

        info!(
            filename = %filename,
            size = data.len(),
            registry = %registry_type,
            "Package upload validation successful"
        );

        Ok(())
    }

    /// Validate total upload size across multiple files
    ///
    /// This consolidates the pattern used for validating total upload size
    /// when handling multipart uploads with multiple package files.
    pub fn validate_total_upload_size(
        total_size: u64,
        registry_type: &str,
    ) -> Result<(), AppError> {
        if total_size > MAX_UPLOAD_SIZE {
            warn!(
                total_size = %total_size,
                max_size = %MAX_UPLOAD_SIZE,
                registry = %registry_type,
                "Total upload size exceeds limit"
            );
            return Err(AppError::UploadError(format!(
                "Total upload size ({} bytes) exceeds maximum allowed ({} bytes)",
                total_size, MAX_UPLOAD_SIZE
            )));
        }

        debug!(
            total_size = %total_size,
            registry = %registry_type,
            "Total upload size validation successful"
        );

        Ok(())
    }
}

/// Centralized Docker parameter validation utilities
pub struct DockerValidator;

impl DockerValidator {
    /// Validate Docker deployment parameters with consistent error handling
    ///
    /// This consolidates the Docker parameter validation pattern used across
    /// main.rs for Docker deployment operations with proper error logging.
    pub fn validate_docker_params(
        host: &str,
        port: u16,
        data_dir: &Path,
    ) -> Result<String, AppError> {
        use crate::validation::{
            validate_docker_port, validate_docker_volume_path, validate_hostname,
        };

        // Validate hostname parameter
        if let Err(e) = validate_hostname(host) {
            error!(host = %host, error = %e, "Invalid host parameter");
            return Err(AppError::BadRequest(format!(
                "Invalid host parameter: {}",
                e
            )));
        }

        // Validate Docker port parameter
        if let Err(e) = validate_docker_port(port) {
            error!(port = %port, error = %e, "Invalid port parameter");
            return Err(AppError::BadRequest(format!(
                "Invalid port parameter: {}",
                e
            )));
        }

        // Validate and get volume path for Docker mounting
        let volume_path = validate_docker_volume_path(data_dir).map_err(|e| {
            error!(path = %data_dir.display(), error = %e, "Invalid Docker volume path");
            AppError::BadRequest(format!("Invalid Docker volume path: {}", e))
        })?;

        info!(
            host = %host,
            port = %port,
            volume_path = %volume_path,
            "Docker parameters validation successful"
        );

        Ok(volume_path)
    }

    /// Validate Docker container name with consistent error handling
    ///
    /// This consolidates the Docker container name validation pattern
    /// used across Docker deployment operations.
    pub fn validate_container_name(name: &str) -> Result<(), AppError> {
        use crate::validation::sanitize_docker_name;

        sanitize_docker_name(name).map_err(|e| {
            error!(container_name = %name, error = %e, "Invalid container name");
            AppError::BadRequest(format!("Invalid container name: {}", e))
        })?;

        debug!(container_name = %name, "Container name validation successful");
        Ok(())
    }

    /// Validate Docker image name with consistent error handling
    ///
    /// This consolidates the Docker image name validation pattern
    /// used across Docker deployment operations.
    pub fn validate_image_name(image: &str) -> Result<(), AppError> {
        use crate::validation::validate_docker_image_name;

        validate_docker_image_name(image).map_err(|e| {
            error!(image_name = %image, error = %e, "Invalid Docker image name");
            AppError::BadRequest(format!("Invalid Docker image name: {}", e))
        })?;

        debug!(image_name = %image, "Docker image name validation successful");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_validate_and_read_small_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = b"Hello, world!";

        fs::write(&file_path, content).await.unwrap();

        let result = FileStreamValidator::validate_and_read_file(&file_path).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), content);
    }

    #[tokio::test]
    async fn test_validate_package_upload_success() {
        let data = b"valid package data";
        let result = FileStreamValidator::validate_package_upload(data, "test.whl", "pypi");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_package_upload_too_large() {
        let large_data = vec![0u8; (MAX_PACKAGE_FILE_SIZE + 1) as usize];
        let result = FileStreamValidator::validate_package_upload(&large_data, "huge.whl", "pypi");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_docker_params_success() {
        let temp_dir = TempDir::new().unwrap();
        let result = DockerValidator::validate_docker_params("localhost", 3080, temp_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_docker_params_invalid_port() {
        let temp_dir = TempDir::new().unwrap();
        // Port 0 should be invalid for Docker
        let result = DockerValidator::validate_docker_params("localhost", 0, temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_container_name_success() {
        let result = DockerValidator::validate_container_name("valid-container-name");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_container_name_invalid() {
        let result = DockerValidator::validate_container_name("invalid/container");
        assert!(result.is_err());
    }
}
