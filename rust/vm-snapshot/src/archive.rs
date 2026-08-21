use std::path::Path;

use rayon::prelude::*;
use vm_core::error::{Result, VmError};
use walkdir::WalkDir;

use crate::manager::snapshot_file_path;
use crate::metadata::SnapshotMetadata;

pub(crate) fn directory_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .par_bridge()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
}

pub(crate) async fn copy_directory(src: &Path, dst: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dst)
        .await
        .map_err(|error| VmError::filesystem(error, dst.display().to_string(), "create_dir_all"))?;

    let mut entries = tokio::fs::read_dir(src)
        .await
        .map_err(|error| VmError::filesystem(error, src.display().to_string(), "read_dir"))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| VmError::general(error, "Failed to read directory entry"))?
    {
        let source = entry.path();
        let destination = dst.join(entry.file_name());
        if source.is_dir() {
            Box::pin(copy_directory(&source, &destination)).await?;
        } else {
            tokio::fs::copy(&source, &destination)
                .await
                .map_err(|error| {
                    VmError::filesystem(error, destination.display().to_string(), "copy")
                })?;
        }
    }

    Ok(())
}

pub(crate) fn create_gzip_archive(
    source: &Path,
    output: &Path,
    compression_level: u8,
) -> Result<()> {
    let temporary_output = temporary_archive_path(output)?;
    let archive_file = std::fs::File::create(&temporary_output).map_err(|error| {
        VmError::filesystem(error, temporary_output.display().to_string(), "create")
    })?;
    let encoder = flate2::write::GzEncoder::new(
        archive_file,
        flate2::Compression::new(compression_level as u32),
    );
    let mut archive = tar::Builder::new(encoder);

    let result = (|| -> Result<()> {
        archive
            .append_dir_all(".", source)
            .map_err(|error| VmError::general(error, "Failed to create tar archive"))?;
        let encoder = archive
            .into_inner()
            .map_err(|error| VmError::general(error, "Failed to finish tar archive"))?;
        encoder
            .finish()
            .map_err(|error| VmError::general(error, "Failed to finish gzip archive"))?;
        std::fs::rename(&temporary_output, output)
            .map_err(|error| VmError::filesystem(error, output.display().to_string(), "rename"))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_output);
    }
    result
}

pub(crate) fn extract_gzip_archive(file_path: &Path, destination: &Path) -> Result<()> {
    let archive_file = std::fs::File::open(file_path)
        .map_err(|error| VmError::filesystem(error, file_path.display().to_string(), "open"))?;
    let decoder = flate2::read::GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    archive.set_overwrite(false);
    archive.set_preserve_permissions(false);

    let entries = archive
        .entries()
        .map_err(|error| VmError::general(error, "Failed to read tar archive entries"))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|error| VmError::general(error, "Failed to read tar archive entry"))?;
        let entry_path = entry
            .path()
            .map_err(|error| VmError::general(error, "Failed to decode tar entry path"))?
            .into_owned();
        if entry_path.is_absolute()
            || entry_path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(VmError::validation(
                format!(
                    "Snapshot contains unsafe path '{}'; refusing to extract",
                    entry_path.display()
                ),
                None::<String>,
            ));
        }

        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(VmError::validation(
                format!(
                    "Snapshot contains symlink or hardlink entry '{}'; refusing to extract",
                    entry_path.display()
                ),
                None::<String>,
            ));
        }
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(VmError::validation(
                format!(
                    "Snapshot contains unsupported archive entry '{}'; only files and directories are allowed",
                    entry_path.display()
                ),
                None::<String>,
            ));
        }
        entry
            .unpack_in(destination)
            .map_err(|error| VmError::general(error, "Failed to extract tar archive"))?;
    }

    Ok(())
}

pub(crate) fn validate_snapshot_files(
    snapshot_dir: &Path,
    metadata: &SnapshotMetadata,
) -> Result<()> {
    let images_dir = snapshot_dir.join("images");
    for service in &metadata.services {
        let image_path = snapshot_file_path(&images_dir, &service.image_file, "image file")?;
        if !image_path.is_file() {
            return Err(VmError::validation(
                format!("Snapshot image file is missing: {}", image_path.display()),
                None::<String>,
            ));
        }
    }

    let volumes_dir = snapshot_dir.join("volumes");
    for volume in &metadata.volumes {
        let archive_path =
            snapshot_file_path(&volumes_dir, &volume.archive_file, "volume archive")?;
        if !archive_path.is_file() {
            return Err(VmError::validation(
                format!(
                    "Snapshot volume archive is missing: {}",
                    archive_path.display()
                ),
                None::<String>,
            ));
        }
    }

    Ok(())
}

fn temporary_archive_path(output: &Path) -> Result<std::path::PathBuf> {
    let mut name = output
        .file_name()
        .map(|name| name.to_os_string())
        .ok_or_else(|| {
            VmError::filesystem(
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing file name"),
                output.display().to_string(),
                "tempname",
            )
        })?;
    name.push(".tmp");
    Ok(output.with_file_name(name))
}
