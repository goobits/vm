use std::path::PathBuf;

use crate::{WorkError, WorkResult};

pub(crate) async fn atomic_write(path: PathBuf, content: Vec<u8>) -> WorkResult<()> {
    tokio::task::spawn_blocking(move || vm_core::file_system::atomic_write(&path, &content))
        .await
        .map_err(|error| WorkError::Internal(format!("atomic write task failed: {error}")))??;
    Ok(())
}
