use crate::error::{AppError, AppResult};
use crate::validation_utils::FileStreamValidator;
use std::path::Path;
use tokio::fs;
use tracing::{debug, info, warn};

/// Save file content to the specified path atomically
pub async fn save_file<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, content: C) -> AppResult<()> {
    let path = path.as_ref();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
        debug!(parent = %parent.display(), "Created parent directory");
    }

    let content = content.as_ref();
    fs::write(path, content).await?;
    info!(
        path = %path.display(),
        size = content.len(),
        "File saved successfully"
    );
    Ok(())
}

/// Read file content from the specified path with size validation
pub async fn read_file<P: AsRef<Path>>(path: P) -> AppResult<Vec<u8>> {
    let path = path.as_ref();

    if !path.exists() {
        warn!(path = %path.display(), "File not found");
        return Err(AppError::NotFound(format!(
            "File not found: {}",
            path.display()
        )));
    }

    // Use centralized validation and file reading logic
    FileStreamValidator::validate_and_read_file(path).await
}

/// Read file content as a string with size validation
pub async fn read_file_string<P: AsRef<Path>>(path: P) -> AppResult<String> {
    let path = path.as_ref();

    // Use centralized validation and string file reading logic
    FileStreamValidator::validate_and_read_file_string(path).await
}

/// Append content to a file, creating it if it doesn't exist, with size validation
pub async fn append_to_file<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, content: C) -> AppResult<()> {
    let path = path.as_ref();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
        debug!(parent = %parent.display(), "Created parent directory");
    }

    let content = content.as_ref();
    let content_str = std::str::from_utf8(content)?;

    // Check existing file size and validate total size after append
    let existing_content = if path.exists() {
        let metadata = fs::metadata(path).await?;
        let existing_size = metadata.len();

        // Use centralized validation for existing file size
        FileStreamValidator::validate_total_upload_size(existing_size, "file append")?;

        // Check if appending would exceed limits
        let total_size = existing_size + content.len() as u64 + 2; // +2 for potential newlines
        FileStreamValidator::validate_total_upload_size(total_size, "file append")?;

        fs::read_to_string(path).await?
    } else {
        // Use centralized validation for new content size
        FileStreamValidator::validate_total_upload_size(content.len() as u64, "file content")?;

        String::new()
    };

    let mut new_content = existing_content;
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(content_str);
    new_content.push('\n');

    fs::write(path, new_content).await?;
    info!(
        path = %path.display(),
        appended_size = content.len(),
        "Content appended to file successfully"
    );
    Ok(())
}
