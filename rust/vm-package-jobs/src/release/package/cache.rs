use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::artifact::BuiltArtifact;

#[derive(Serialize, Deserialize)]
struct Receipt {
    commit: String,
    filename: String,
    digest: String,
}

pub(super) struct ArtifactCache {
    directory: PathBuf,
    _lock: fs::File,
}

impl ArtifactCache {
    pub(super) fn open(root: &Path, submission: &str) -> Result<Self> {
        vm_packages::validate_managed_id("submission ID", submission)?;
        fs::create_dir_all(root)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(format!("{submission}.lock")))?;
        fs2::FileExt::lock_exclusive(&lock)?;
        Ok(Self {
            directory: root.join(submission),
            _lock: lock,
        })
    }

    pub(super) fn artifact(
        &self,
        commit: &str,
        build: impl FnOnce() -> Result<BuiltArtifact>,
    ) -> Result<BuiltArtifact> {
        let receipt_path = self.directory.join("receipt.json");
        if receipt_path.try_exists()? {
            let receipt: Receipt = serde_json::from_slice(&fs::read(receipt_path)?)?;
            validate_filename(&receipt.filename)?;
            let path = self.directory.join(&receipt.filename);
            let file = fs::File::open(&path)?;
            let digest = vm_packages::sha256_reader(std::io::BufReader::new(file))?.0;
            if receipt.commit != commit || receipt.digest != digest {
                bail!("retained package artifact does not match its immutable release receipt");
            }
            return Ok(BuiltArtifact { path, digest });
        }
        // An interrupted initial write has no published receipt yet. Rebuild
        // only that submission's incomplete staging directory.
        self.cleanup()?;
        let artifact = build()?;
        let filename = artifact
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .context("package artifact has no UTF-8 filename")?;
        validate_filename(filename)?;
        fs::create_dir_all(&self.directory)?;
        let path = self.directory.join(filename);
        fs::copy(&artifact.path, &path)?;
        fs::File::open(&path)?.sync_all()?;
        let digest = vm_packages::sha256_reader(std::io::BufReader::new(fs::File::open(&path)?))?.0;
        if digest != artifact.digest {
            bail!("package artifact changed while it was retained");
        }
        let receipt = Receipt {
            commit: commit.into(),
            filename: filename.into(),
            digest: artifact.digest.clone(),
        };
        vm_core::file_system::atomic_write(&receipt_path, &serde_json::to_vec(&receipt)?)?;
        Ok(BuiltArtifact {
            path,
            digest: artifact.digest,
        })
    }

    pub(super) fn cleanup(&self) -> Result<()> {
        match fs::remove_dir_all(&self.directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn validate_filename(filename: &str) -> Result<()> {
    let mut components = Path::new(filename).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
        || filename.contains(['\\', '\0', '\n', '\r'])
    {
        bail!("package artifact filename must be one path component");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_publication_reuses_exact_bytes_and_detects_corruption() {
        let root = tempfile::tempdir().unwrap();
        let artifact = root.path().join("shared-1.0.0.tgz");
        fs::write(&artifact, "first build").unwrap();
        let cache = ArtifactCache::open(&root.path().join("cache"), "sub-1").unwrap();
        let saved = cache
            .artifact("commit-1", || {
                Ok(BuiltArtifact {
                    path: artifact,
                    digest: vm_packages::sha256_hex("first build"),
                })
            })
            .unwrap();
        drop(cache);
        let cache = ArtifactCache::open(&root.path().join("cache"), "sub-1").unwrap();
        let resumed = cache
            .artifact("commit-1", || {
                panic!("must not rebuild a retained artifact")
            })
            .unwrap();
        assert_eq!(resumed.digest, saved.digest);
        assert_eq!(fs::read(&resumed.path).unwrap(), b"first build");
        assert!(cache.artifact("commit-2", || unreachable!()).is_err());
        fs::write(&resumed.path, "changed").unwrap();
        assert!(cache.artifact("commit-1", || unreachable!()).is_err());
        cache.cleanup().unwrap();
        cache.cleanup().unwrap();
        assert!(!saved.path.exists());
    }
}
