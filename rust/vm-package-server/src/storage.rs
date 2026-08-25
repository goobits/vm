use crate::error::{AppError, AppResult};
use crate::validation;
use std::future::Future;
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, MutexGuard};
use tracing::{debug, warn};

static PUBLISH_LOCK: Mutex<()> = Mutex::const_new(());
pub const METADATA_CACHE_TTL: Duration = Duration::from_secs(5);

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
                if let Err(cleanup_error) = fs::remove_file(path).await {
                    warn!(
                        operation = "cleanup_failed_immutable_write",
                        path = %path.display(),
                        error = ?cleanup_error,
                        write_error = ?error,
                        "incomplete immutable artifact cleanup failed"
                    );
                }
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
    }

    let content = content.as_ref().to_vec();
    let content_len = content.len();
    let owned_path = path.to_path_buf();
    tokio::task::spawn_blocking(move || vm_core::file_system::atomic_write(&owned_path, &content))
        .await
        .map_err(|error| AppError::InternalError(format!("atomic write task failed: {error}")))??;
    debug!(
        operation = "write_file",
        path = %path.display(),
        size = content_len,
        "registry file saved"
    );
    Ok(())
}

/// Read file content from the specified path with size validation
pub async fn read_file<P: AsRef<Path>>(path: P) -> AppResult<Vec<u8>> {
    let path = path.as_ref();
    validate_read_size(path).await?;
    Ok(fs::read(path).await?)
}

/// Read an immutable artifact locally, then from a persistent read-through cache.
pub async fn read_local_or_cache<L, C, F, Fut>(
    local_path: L,
    cache_path: C,
    fetch: F,
) -> AppResult<Vec<u8>>
where
    L: AsRef<Path>,
    C: AsRef<Path>,
    F: FnOnce() -> Fut,
    Fut: Future<Output = AppResult<Vec<u8>>>,
{
    match read_file(local_path).await {
        Ok(content) => Ok(content),
        Err(AppError::NotFound(_)) => read_through_cache(cache_path, fetch).await,
        Err(error) => Err(error),
    }
}

/// Fetch and persist an immutable artifact when it is absent from the cache.
pub async fn read_through_cache<P, F, Fut>(path: P, fetch: F) -> AppResult<Vec<u8>>
where
    P: AsRef<Path>,
    F: FnOnce() -> Fut,
    Fut: Future<Output = AppResult<Vec<u8>>>,
{
    let path = path.as_ref();
    match read_file(path).await {
        Ok(content) => return Ok(content),
        Err(AppError::NotFound(_)) => {}
        Err(error) => return Err(error),
    }

    let content = fetch().await?;
    save_immutable(path, &content).await?;
    Ok(content)
}

/// Refresh mutable registry metadata, falling back to the last valid snapshot.
pub async fn read_refreshing_cache<P, F, Fut>(
    path: P,
    max_age: Duration,
    fetch: F,
) -> AppResult<Vec<u8>>
where
    P: AsRef<Path>,
    F: FnOnce() -> Fut,
    Fut: Future<Output = AppResult<Vec<u8>>>,
{
    let path = path.as_ref();
    if cache_is_fresh(path, max_age).await? {
        return read_file(path).await;
    }

    match fetch().await {
        Ok(content) => {
            if let Err(error) = save_file(path, &content).await {
                warn!(
                    operation = "refresh_cache",
                    path = %path.display(),
                    error = %error,
                    "registry metadata cache refresh failed"
                );
            }
            Ok(content)
        }
        Err(fetch_error) => match read_file(path).await {
            Ok(content) => {
                warn!(
                    operation = "refresh_cache",
                    path = %path.display(),
                    error = %fetch_error,
                    outcome = "using_stale",
                    "registry metadata refresh failed"
                );
                Ok(content)
            }
            Err(AppError::NotFound(_)) => Err(fetch_error),
            Err(cache_error) => Err(cache_error),
        },
    }
}

async fn cache_is_fresh(path: &Path, max_age: Duration) -> AppResult<bool> {
    let metadata = match fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(metadata
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age <= max_age))
}

/// Read file content as a string with size validation
pub async fn read_file_string<P: AsRef<Path>>(path: P) -> AppResult<String> {
    let path = path.as_ref();
    validate_read_size(path).await?;
    Ok(fs::read_to_string(path).await?)
}

async fn validate_read_size(path: &Path) -> AppResult<()> {
    let metadata = fs::metadata(path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(format!("File not found: {}", path.display()))
        } else {
            error.into()
        }
    })?;
    validation::validate_file_size(metadata.len(), Some(validation::MAX_UPLOAD_SIZE))
        .map_err(|error| AppError::BadRequest(format!("File too large: {error}")))
}

/// Append content to a file, creating it if it doesn't exist, with size validation
pub async fn append_to_file<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, content: C) -> AppResult<()> {
    let path = path.as_ref();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let content = content.as_ref();
    let content_str = std::str::from_utf8(content)?;

    // Check existing file size and validate total size after append
    let existing_content = if path.exists() {
        let metadata = fs::metadata(path).await?;
        let existing_size = metadata.len();

        // Use centralized validation for existing file size
        validation::validate_total_upload_size(existing_size, "file append")?;

        // Check if appending would exceed limits
        let total_size = existing_size + content.len() as u64 + 2; // +2 for potential newlines
        validation::validate_total_upload_size(total_size, "file append")?;

        fs::read_to_string(path).await?
    } else {
        // Use centralized validation for new content size
        validation::validate_total_upload_size(content.len() as u64, "file content")?;

        String::new()
    };

    let mut new_content = existing_content;
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(content_str);
    new_content.push('\n');

    save_file(path, new_content.as_bytes()).await?;
    debug!(
        operation = "append_file",
        path = %path.display(),
        appended_size = content.len(),
        "registry file appended"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        append_to_file, read_file, read_local_or_cache, read_refreshing_cache, save_file,
        save_immutable,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;

    use crate::AppError;

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

    #[tokio::test]
    async fn read_through_cache_fetches_an_artifact_once() {
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("published/package.tgz");
        let cached = directory.path().join("cache/package.tgz");
        let fetches = Arc::new(AtomicUsize::new(0));

        for _ in 0..2 {
            let fetches = Arc::clone(&fetches);
            let content = read_local_or_cache(&local, &cached, move || async move {
                fetches.fetch_add(1, Ordering::SeqCst);
                Ok(b"upstream".to_vec())
            })
            .await
            .unwrap();
            assert_eq!(content, b"upstream");
        }

        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        assert_eq!(tokio::fs::read(cached).await.unwrap(), b"upstream");
    }

    #[tokio::test]
    async fn refreshing_cache_uses_fresh_then_stale_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let cached = directory.path().join("metadata/index.json");
        let fetches = Arc::new(AtomicUsize::new(0));
        let fetched = Arc::clone(&fetches);
        let content = read_refreshing_cache(&cached, Duration::from_secs(60), move || async move {
            fetched.fetch_add(1, Ordering::SeqCst);
            Ok(b"fresh".to_vec())
        })
        .await
        .unwrap();
        assert_eq!(content, b"fresh");

        let fetched = Arc::clone(&fetches);
        let content = read_refreshing_cache(&cached, Duration::from_secs(60), move || async move {
            fetched.fetch_add(1, Ordering::SeqCst);
            Ok(b"unexpected".to_vec())
        })
        .await
        .unwrap();
        assert_eq!(content, b"fresh");

        let content = read_refreshing_cache(&cached, Duration::ZERO, || async {
            Err(AppError::Unavailable("offline".into()))
        })
        .await
        .unwrap();
        assert_eq!(content, b"fresh");
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn storage_reports_missing_files_and_creates_parent_directories() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");
        assert!(matches!(
            read_file(&missing).await,
            Err(crate::AppError::NotFound(_))
        ));

        let nested = directory.path().join("nested/package/index");
        save_file(&nested, b"metadata").await.unwrap();
        assert_eq!(read_file(nested).await.unwrap(), b"metadata");
    }

    #[tokio::test]
    async fn append_is_line_oriented_and_rejects_non_utf8() {
        let directory = tempfile::tempdir().unwrap();
        let index = directory.path().join("index");
        append_to_file(&index, b"one").await.unwrap();
        append_to_file(&index, b"two").await.unwrap();
        assert_eq!(
            tokio::fs::read_to_string(&index).await.unwrap(),
            "one\ntwo\n"
        );

        assert!(matches!(
            append_to_file(&index, [0xff]).await,
            Err(crate::AppError::Utf8(_))
        ));
    }
}
