use crate::error::{AppError, AppResult};
use crate::validation_utils::FileStreamValidator;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, MutexGuard};
use tracing::{debug, info, warn};

static PUBLISH_LOCK: Mutex<()> = Mutex::const_new(());

/// Serialize release commits so multi-file registry metadata remains deterministic.
pub async fn publish_guard() -> MutexGuard<'static, ()> {
    PUBLISH_LOCK.lock().await
}

/// Create an immutable artifact, accepting an exact retry and rejecting replacement.
pub async fn save_immutable<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, content: C) -> AppResult<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let content = content.as_ref();
    let opened = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .await;
    match opened {
        Ok(mut file) => {
            if let Err(error) = file.write_all(content).await {
                drop(file);
                let _ = fs::remove_file(path).await;
                return Err(error.into());
            }
            file.sync_all().await?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if fs::read(path).await? == content {
                Ok(())
            } else {
                Err(AppError::Conflict(format!(
                    "immutable artifact '{}' already exists with different content",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("package")
                )))
            }
        }
        Err(error) => Err(error.into()),
    }
}

/// Save file content to the specified path atomically
pub async fn save_file<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, content: C) -> AppResult<()> {
    let path = path.as_ref();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
        debug!(parent = %parent.display(), "Created parent directory");
    }

    let content = content.as_ref().to_vec();
    let content_len = content.len();
    let owned_path = path.to_path_buf();
    tokio::task::spawn_blocking(move || vm_core::file_system::atomic_write(&owned_path, &content))
        .await
        .map_err(|error| AppError::InternalError(format!("atomic write task failed: {error}")))??;
    info!(
        path = %path.display(),
        size = content_len,
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

    save_file(path, new_content.as_bytes()).await?;
    info!(
        path = %path.display(),
        appended_size = content.len(),
        "Content appended to file successfully"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::save_immutable;

    #[tokio::test]
    async fn immutable_artifacts_allow_exact_retries_only() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("auth-1.0.0.tgz");

        save_immutable(&artifact, b"first").await.unwrap();
        save_immutable(&artifact, b"first").await.unwrap();
        let error = save_immutable(&artifact, b"changed").await.unwrap_err();

        assert!(matches!(error, crate::AppError::Conflict(_)));
        assert_eq!(tokio::fs::read(artifact).await.unwrap(), b"first");
    }
}
