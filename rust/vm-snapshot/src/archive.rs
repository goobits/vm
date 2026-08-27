use std::path::Path;

use vm_core::error::{Result, VmError};
use walkdir::WalkDir;

use crate::manager::snapshot_file_path;
use crate::metadata::SnapshotMetadata;

const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024 * 1024;

pub(crate) fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0_u64;
    for entry in WalkDir::new(path) {
        let entry = entry.map_err(|error| {
            VmError::general(
                error,
                format!("Failed to walk snapshot directory {}", path.display()),
            )
        })?;
        if entry.file_type().is_file() {
            let metadata = entry.metadata().map_err(|error| {
                VmError::general(
                    error,
                    format!("Failed to inspect snapshot file {}", entry.path().display()),
                )
            })?;
            total = total.checked_add(metadata.len()).ok_or_else(|| {
                VmError::validation("Snapshot size exceeds supported range", None::<String>)
            })?;
        }
    }
    Ok(total)
}

pub(crate) async fn copy_directory(src: &Path, dst: &Path) -> Result<()> {
    let source_metadata = tokio::fs::symlink_metadata(src).await.map_err(|error| {
        VmError::filesystem(error, src.display().to_string(), "symlink_metadata")
    })?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_dir() {
        return Err(VmError::validation(
            format!("Snapshot source is not a real directory: {}", src.display()),
            None::<String>,
        ));
    }
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
        let metadata = tokio::fs::symlink_metadata(&source)
            .await
            .map_err(|error| {
                VmError::filesystem(error, source.display().to_string(), "symlink_metadata")
            })?;
        if metadata.file_type().is_symlink() {
            return Err(VmError::validation(
                format!(
                    "Snapshot source contains symlink '{}'; refusing to copy",
                    source.display()
                ),
                None::<String>,
            ));
        }
        if metadata.is_dir() {
            Box::pin(copy_directory(&source, &destination)).await?;
        } else if metadata.is_file() {
            tokio::fs::copy(&source, &destination)
                .await
                .map_err(|error| {
                    VmError::filesystem(error, destination.display().to_string(), "copy")
                })?;
        } else {
            return Err(VmError::validation(
                format!(
                    "Snapshot source contains unsupported file '{}'; only files and directories are allowed",
                    source.display()
                ),
                None::<String>,
            ));
        }
    }

    Ok(())
}

pub(crate) fn create_gzip_archive(
    source: &Path,
    output: &Path,
    compression_level: u8,
) -> Result<()> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary_output = tempfile::Builder::new()
        .prefix(".vm-snapshot-")
        .tempfile_in(parent)
        .map_err(|error| VmError::filesystem(error, parent.display().to_string(), "tempfile"))?;
    let archive_file = temporary_output.reopen().map_err(|error| {
        VmError::filesystem(
            error,
            temporary_output.path().display().to_string(),
            "reopen",
        )
    })?;
    let encoder = flate2::write::GzEncoder::new(
        archive_file,
        flate2::Compression::new(compression_level as u32),
    );
    let mut archive = tar::Builder::new(encoder);

    archive
        .append_dir_all(".", source)
        .map_err(|error| VmError::general(error, "Failed to create tar archive"))?;
    let encoder = archive
        .into_inner()
        .map_err(|error| VmError::general(error, "Failed to finish tar archive"))?;
    let archive_file = encoder
        .finish()
        .map_err(|error| VmError::general(error, "Failed to finish gzip archive"))?;
    archive_file
        .sync_all()
        .map_err(|error| VmError::filesystem(error, output.display().to_string(), "sync_all"))?;
    temporary_output.persist(output).map_err(|error| {
        VmError::filesystem(error.error, output.display().to_string(), "persist")
    })?;
    Ok(())
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
    let mut count = 0_usize;
    let mut unpacked = 0_u64;
    for entry in entries {
        let mut entry =
            entry.map_err(|error| VmError::general(error, "Failed to read tar archive entry"))?;
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err(VmError::validation(
                format!("Snapshot exceeds the {MAX_ARCHIVE_ENTRIES} entry limit"),
                None::<String>,
            ));
        }
        unpacked = unpacked.checked_add(entry.size()).ok_or_else(|| {
            VmError::validation(
                "Snapshot unpacked size exceeds supported range",
                None::<String>,
            )
        })?;
        if unpacked > MAX_UNPACKED_BYTES {
            return Err(VmError::validation(
                format!("Snapshot exceeds the {MAX_UNPACKED_BYTES} byte unpacked limit"),
                None::<String>,
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::{copy_directory, create_gzip_archive, directory_size};

    #[test]
    fn directory_size_counts_every_file_and_reports_missing_roots() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("nested")).unwrap();
        std::fs::write(directory.path().join("one"), b"123").unwrap();
        std::fs::write(directory.path().join("nested/two"), b"4567").unwrap();

        assert_eq!(directory_size(directory.path()).unwrap(), 7);
        assert!(directory_size(&directory.path().join("missing")).is_err());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn copy_directory_rejects_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let destination = directory.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(directory.path().join("outside"), "owner-data").unwrap();
        std::os::unix::fs::symlink(directory.path().join("outside"), source.join("link")).unwrap();

        assert!(copy_directory(&source, &destination).await.is_err());
        assert!(!destination.join("link").exists());
    }

    #[test]
    #[cfg(unix)]
    fn archive_creation_does_not_follow_a_predictable_temp_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let output = directory.path().join("snapshot.tar.gz");
        let victim = directory.path().join("victim");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("file"), "snapshot").unwrap();
        std::fs::write(&victim, "owner-data").unwrap();
        std::os::unix::fs::symlink(&victim, directory.path().join("snapshot.tar.gz.tmp")).unwrap();

        create_gzip_archive(&source, &output, 1).unwrap();

        assert_eq!(std::fs::read_to_string(victim).unwrap(), "owner-data");
        assert!(output.is_file());
    }
}
