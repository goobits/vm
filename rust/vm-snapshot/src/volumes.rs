use std::path::Path;

use futures_util::stream::{self, StreamExt};
use vm_core::error::{Result, VmError};

use crate::compose_plan::NamedVolume;
use crate::docker::{execute_docker_streaming, execute_docker_with_output};
use crate::metadata::VolumeSnapshot;
use crate::optimal_concurrency;

pub(crate) async fn verify_owned_volumes(executable: &str, volumes: &[NamedVolume]) -> Result<()> {
    for volume in volumes {
        let output =
            execute_docker_with_output(executable, &["volume", "inspect", &volume.runtime_name])
                .await?;
        let inspected: serde_json::Value = serde_json::from_str(&output)
            .map_err(|error| VmError::general(error, "Cannot inspect snapshot volume ownership"))?;
        let labels = &inspected[0]["Labels"];
        if labels["com.vm.managed"] != "true"
            || labels["com.vm.instance"] != volume.instance
            || labels["com.vm.config-path"] != volume.owner_config_path
            || labels["com.vm.scope"] != "instance"
        {
            return Err(VmError::validation(
                format!("Volume '{}' has a different owner", volume.runtime_name),
                None::<String>,
            ));
        }
    }
    Ok(())
}

pub(crate) async fn backup_volumes(
    executable: &str,
    volumes_dir: &Path,
    volumes: &[NamedVolume],
) -> Result<Vec<VolumeSnapshot>> {
    let backup_futures = volumes.iter().map(|volume| {
        let volume = volume.clone();
        let volumes_dir = volumes_dir.to_path_buf();
        async move {
            tracing::info!("  Backing up volume: {}", volume.name);
            let archive_file = format!("{}.tar.gz", volume.name);
            let archive_path = volumes_dir.join(&archive_file);
            // The helper runs as root to read volume data. Create its output
            // as the caller first so native Linux backups remain caller-owned
            // and private, rather than inheriting the container's ownership.
            let mut archive_options = tokio::fs::OpenOptions::new();
            archive_options.write(true).create_new(true);
            #[cfg(unix)]
            archive_options.mode(0o600);
            drop(archive_options.open(&archive_path).await.map_err(|error| {
                VmError::filesystem(error, archive_path.to_string_lossy(), "create")
            })?);
            let run_args = [
                "run",
                "--rm",
                "-v",
                &format!("{}:/data:ro", volume.runtime_name),
                "-v",
                &format!("{}:/backup", volumes_dir.to_string_lossy()),
                "alpine:latest",
                "sh",
                "-c",
                &format!("tar -czf /backup/{archive_file} -C /data ."),
            ];
            execute_docker_with_output(executable, &run_args).await?;
            let size_bytes = tokio::fs::metadata(&archive_path)
                .await
                .map_err(|error| {
                    VmError::filesystem(error, archive_path.to_string_lossy(), "metadata")
                })?
                .len();
            Ok::<_, VmError>(VolumeSnapshot {
                name: volume.name,
                runtime_name: volume.runtime_name,
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
    volumes_dir: &Path,
    volumes: &[VolumeSnapshot],
) -> Result<()> {
    let restore_futures = volumes.iter().map(|volume| {
        let volume = volume.clone();
        let volumes_dir = volumes_dir.to_path_buf();
        async move {
            tracing::info!("  Restoring volume: {}", volume.name);
            let full_volume_name = &volume.runtime_name;
            // Keep the existing, ownership-verified volume and its labels.
            // Delete the captured state only after validating the archive.
            let restore_command = "tar -tzf \"/backup/$1\" >/dev/null && find /data -mindepth 1 -maxdepth 1 -exec rm -rf -- {} + && tar -xzf \"/backup/$1\" -C /data";
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
