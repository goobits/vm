use std::path::Path;

use futures_util::stream::{self, StreamExt};
use vm_core::error::{Result, VmError};

use crate::docker::{
    execute_docker, execute_docker_streaming, execute_docker_with_output,
    remove_docker_volume_if_present,
};
use crate::metadata::VolumeSnapshot;
use crate::optimal_concurrency;

pub(crate) async fn backup_volumes(
    executable: &str,
    project_name: &str,
    volumes_dir: &Path,
    volume_names: &[String],
) -> Result<Vec<VolumeSnapshot>> {
    let backup_futures = volume_names.iter().map(|volume| {
        let volume = volume.clone();
        let volumes_dir = volumes_dir.to_path_buf();
        async move {
            tracing::info!("  Backing up volume: {}", volume);
            let archive_file = format!("{volume}.tar.zst");
            let archive_path = volumes_dir.join(&archive_file);
            let full_volume_name = format!("{project_name}_{volume}");
            let run_args = [
                "run",
                "--rm",
                "-v",
                &format!("{full_volume_name}:/data"),
                "-v",
                &format!("{}:/backup", volumes_dir.to_string_lossy()),
                "alpine:latest",
                "sh",
                "-c",
                &format!("tar -c -C /data . | zstd -3 -T0 > /backup/{archive_file}"),
            ];
            execute_docker_with_output(executable, &run_args).await?;
            let size_bytes = tokio::fs::metadata(&archive_path)
                .await
                .map_err(|error| {
                    VmError::filesystem(error, archive_path.to_string_lossy(), "metadata")
                })?
                .len();
            Ok::<_, VmError>(VolumeSnapshot {
                name: volume,
                archive_file,
                size_bytes,
            })
        }
    });

    stream::iter(backup_futures)
        .buffer_unordered(optimal_concurrency())
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect()
}

pub(crate) async fn restore_volumes(
    executable: &str,
    project_name: &str,
    volumes_dir: &Path,
    volumes: &[VolumeSnapshot],
    force: bool,
) -> Result<()> {
    let restore_futures = volumes.iter().map(|volume| {
        let volume = volume.clone();
        let volumes_dir = volumes_dir.to_path_buf();
        async move {
            tracing::info!("  Restoring volume: {}", volume.name);
            let full_volume_name = format!("{project_name}_{}", volume.name);
            if force {
                remove_docker_volume_if_present(executable, &full_volume_name).await?;
            }
            execute_docker(executable, &["volume", "create", &full_volume_name]).await?;

            let restore_command = if volume.archive_file.ends_with(".tar.zst") {
                "zstd -d -c \"/backup/$1\" | tar -x -C /data"
            } else {
                "tar -xzf \"/backup/$1\" -C /data"
            };
            let run_args = [
                "run",
                "--rm",
                "-v",
                &format!("{full_volume_name}:/data"),
                "-v",
                &format!("{}:/backup", volumes_dir.to_string_lossy()),
                "alpine:latest",
                "sh",
                "-c",
                restore_command,
                "snapshot-restore",
                &volume.archive_file,
            ];
            execute_docker_streaming(executable, &run_args).await
        }
    });

    stream::iter(restore_futures)
        .buffer_unordered(optimal_concurrency())
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    Ok(())
}
