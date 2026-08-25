use std::path::Path;

pub(crate) async fn cleanup_file(path: &Path, operation: &'static str) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(
            operation,
            path = %path.display(),
            error = ?error,
            "managed temporary file cleanup failed"
        ),
    }
}

pub(crate) async fn cleanup_directory(path: &Path, operation: &'static str) {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(
            operation,
            path = %path.display(),
            error = ?error,
            "managed temporary directory cleanup failed"
        ),
    }
}
