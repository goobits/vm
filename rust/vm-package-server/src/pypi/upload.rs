use super::*;

/// Uploads a new PyPI package to the repository.
///
/// Accepts multipart form data containing package files (.whl, .tar.gz) and stores them
/// in the PyPI packages directory. Generates SHA256 hashes for integrity verification.
///
/// # Route
/// `POST /pypi/`
///
/// # Request Body
/// Multipart form data with package files
///
/// # Returns
/// JSON success response with upload confirmation
///
/// # Supported Formats
/// - `.whl` files (Python wheels)
/// - `.tar.gz` files (source distributions)
///
/// # Example Response
/// ```json
/// {
///   "message": "Package uploaded successfully",
///   "status": "success"
/// }
/// ```
pub async fn upload_package(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> AppResult<axum::Json<SuccessResponse>> {
    crate::auth::validate_publish_headers(&state.config, &headers)?;

    let pypi_dir = state.data_dir.join("pypi/packages");

    let mut field_count = 0;
    let mut total_size = 0u64;

    while let Some(field) = multipart.next_field().await? {
        field_count += 1;
        let name = field.name().unwrap_or("").to_string();

        // Validate multipart limits early to prevent resource exhaustion
        if field_count > validation::MAX_MULTIPART_FIELDS {
            return Err(AppError::UploadError(format!(
                "Too many multipart fields: {} (max: {})",
                field_count,
                validation::MAX_MULTIPART_FIELDS
            )));
        }

        if name == "content" {
            let filename = field
                .file_name()
                .ok_or_else(|| AppError::BadRequest("Missing filename in upload".to_string()))?
                .to_string();

            // Validate filename for security
            crate::validation::validate_filename(&filename)?;

            // Only accept .whl and .tar.gz files
            if !package_utils::validate_file_extension(&filename, &[".whl", ".tar.gz"]) {
                return Err(AppError::BadRequest(
                    "Only .whl and .tar.gz files are allowed".to_string(),
                ));
            }

            // Read the data with size constraints to prevent memory exhaustion
            let data = field.bytes().await?;
            // Use centralized validation for package uploads
            validation::validate_package_upload(&data, &filename, "PyPI")?;

            total_size += data.len() as u64;

            // Validate total multipart upload size
            validation::validate_multipart_limits(field_count, total_size, None).map_err(|e| {
                AppError::UploadError(format!("Multipart upload limits exceeded: {e}"))
            })?;

            // Calculate hash once during upload
            let hash = sha256_hex(&data);
            let _publish_guard = storage::publish_guard().await;

            // Immutable release files accept exact retries but never replacement.
            let file_path = pypi_dir.join(&filename);
            storage::save_immutable(&file_path, &data).await?;

            // Save the hash to a .meta file
            let meta_path = file_path.with_extension(format!(
                "{}.meta",
                file_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
            ));
            storage::save_immutable(meta_path, hash.as_bytes()).await?;

            info!(
                operation = "publish",
                ecosystem = "python",
                filename = %filename,
                size = data.len(),
                outcome = "published",
                "package publication completed"
            );
            return Ok(axum::Json(SuccessResponse {
                message: "Upload successful".to_string(),
            }));
        } else {
            // For non-content fields, we still need to read and count them for size validation
            let field_data = field.bytes().await?;
            total_size += field_data.len() as u64;

            // Use centralized validation for total upload size
            validation::validate_total_upload_size(total_size, "PyPI")?;
        }
    }

    Err(AppError::BadRequest("No content field found".to_string()))
}
