//! Ownership proof for the executable installed by vm-installer.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = ".vm-install.json";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallRecord {
    pub executable: PathBuf,
    pub version: String,
    pub sha256: String,
}

pub fn path_for(executable: &Path) -> io::Result<PathBuf> {
    let parent = executable
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Executable has no parent"))?;
    Ok(parent.join(FILE_NAME))
}

pub fn digest(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
    }
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(hex)
}

pub fn verified(executable: &Path) -> io::Result<InstallRecord> {
    let executable = fs::canonicalize(executable)?;
    let metadata = fs::symlink_metadata(&executable)?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Installation executable is not a regular file",
        ));
    }
    let path = path_for(&executable)?;
    let record: InstallRecord = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if record.executable != executable || record.sha256 != digest(&executable)? {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Installation record does not match this executable",
        ));
    }
    Ok(record)
}

pub fn write(executable: &Path, version: &str) -> io::Result<()> {
    let executable = fs::canonicalize(executable)?;
    let record = InstallRecord {
        sha256: digest(&executable)?,
        executable: executable.clone(),
        version: version.to_owned(),
    };
    let path = path_for(&executable)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Record has no parent"))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(staged.as_file_mut(), &record)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;
    staged.as_file_mut().write_all(b"\n")?;
    staged.as_file_mut().sync_all()?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_binds_exact_file_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("vm");
        fs::write(&executable, b"first").unwrap();
        write(&executable, "1.2.3").unwrap();
        assert_eq!(verified(&executable).unwrap().version, "1.2.3");
        fs::write(&executable, b"changed").unwrap();
        assert!(verified(&executable).is_err());
    }
}
