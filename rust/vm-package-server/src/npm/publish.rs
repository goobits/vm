use super::*;

/// Publishes a new NPM package version to the local registry.
///
/// This endpoint handles NPM package publishing according to the NPM registry API.
/// It processes multipart uploads containing package metadata and tarball data,
/// validates the content, and stores both the tarball and metadata.
///
/// # Route
/// `PUT /npm/{package}`
///
/// # Parameters
/// * `package` - The NPM package name to publish
/// * `payload` - JSON payload containing package metadata and base64-encoded tarball
///
/// # Payload Structure
/// ```json
/// {
///   "_id": "package-name",
///   "name": "package-name",
///   "versions": {
///     "1.0.0": {
///       "name": "package-name",
///       "version": "1.0.0",
///       "dist": {
///         "tarball": "http://server/npm/package/-/package-1.0.0.tgz"
///       }
///     }
///   },
///   "_attachments": {
///     "package-1.0.0.tgz": {
///       "data": "base64-encoded-tarball",
///       "content_type": "application/octet-stream"
///     }
///   }
/// }
/// ```
///
/// # Returns
/// JSON success response confirming package publication
///
/// # Processing Steps
/// 1. Extracts and validates `_attachments` field
/// 2. Decodes base64 tarball data
/// 3. Calculates SHA1 hash for integrity
/// 4. Saves tarball to `npm/tarballs/` directory
/// 5. Updates metadata with calculated hash
/// 6. Saves metadata to `npm/metadata/` directory
///
/// # Error Conditions
/// - Missing or invalid `_attachments` field
/// - No valid .tgz attachment found
/// - Base64 decoding failures
/// - File system write errors
pub async fn publish_package(
    AxumPath(package): AxumPath<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut payload): Json<Value>,
) -> AppResult<Json<SuccessResponse>> {
    crate::auth::validate_publish_headers(&state.config, &headers)?;
    let metadata_path = metadata_path(&state.data_dir, &package)?;
    if payload["name"].as_str() != Some(package.as_str()) {
        return Err(AppError::BadRequest(
            "npm route package and payload name must match".to_string(),
        ));
    }

    // Extract attachments containing the tarball
    let attachments = payload["_attachments"]
        .as_object()
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "Package '{package}': '_attachments' field is not an object"
            ))
        })?
        .clone();

    for (filename, attachment) in &attachments {
        if filename.ends_with(".tgz") {
            crate::validation::validate_filename(filename)?;

            let data_b64 = attachment["data"].as_str().ok_or_else(|| {
                AppError::UploadError("Attachment 'data' field is not a string".to_string())
            })?;

            debug!(filename = %filename, "Validating and decoding base64 tarball data");

            // Comprehensive validation before processing base64 data
            validation::validate_base64_size(data_b64, None, None).map_err(|e| {
                AppError::UploadError(format!("Base64 data size validation failed: {e}"))
            })?;

            // Validate base64 character format
            validation::validate_base64_characters(data_b64)
                .map_err(|e| AppError::UploadError(format!("Invalid base64 format: {e}")))?;

            // Decode base64 tarball with comprehensive error handling
            let tarball_data = general_purpose::STANDARD
                .decode(data_b64)
                .map_err(|e| AppError::UploadError(format!("Invalid base64 encoding: {e}")))?;

            // Use centralized validation for decoded tarball size
            validation::validate_package_upload(&tarball_data, filename, "NPM")?;

            // Calculate SHA1 hash for metadata
            let shasum = sha1_hex(&tarball_data);

            // Update metadata with correct tarball URL and hash
            if let Some(versions) = payload["versions"].as_object_mut() {
                for version_data in versions.values_mut() {
                    if let Some(dist) = version_data.get_mut("dist").and_then(|d| d.as_object_mut())
                    {
                        dist.insert("shasum".to_string(), json!(shasum));
                    }
                }
            }

            // Remove attachments before saving metadata
            if let Some(obj) = payload.as_object_mut() {
                obj.remove("_attachments");
            }

            let _publish_guard = storage::publish_guard().await;
            let merged = merge_metadata(&metadata_path, &payload).await?;

            // Commit the immutable artifact before its discoverable metadata.
            let tarball_path = state.data_dir.join("npm/tarballs").join(filename);
            storage::save_immutable(tarball_path, &tarball_data).await?;

            // Save metadata
            let metadata_str = serde_json::to_string_pretty(&merged)?;
            storage::save_file(metadata_path, metadata_str.as_bytes()).await?;

            info!(
                operation = "publish",
                ecosystem = "npm",
                package = %package,
                filename = %filename,
                size = tarball_data.len(),
                outcome = "published",
                "package publication completed"
            );
            return Ok(Json(SuccessResponse {
                message: "Package published successfully".to_string(),
            }));
        }
    }

    Err(AppError::UploadError(
        "No valid .tgz attachment found".to_string(),
    ))
}
