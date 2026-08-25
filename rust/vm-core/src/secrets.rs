//! Secrets management for VM services
//!
//! This module provides functionality for generating and retrieving
//! passwords for VM services in a secure manner.

use rand::prelude::*;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::user_paths::secrets_dir;

/// Generate a random password.
pub fn generate_random_password(length: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789";
    let mut rng = rand::rng();
    (0..length)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Get a password from the secrets store, or generate a new one if it doesn't exist.
///
/// This is a synchronous version that's suitable for use in non-async contexts
/// like template rendering.
pub fn get_or_generate_password_sync(service_name: &str) -> std::io::Result<String> {
    validate_service_name(service_name)?;
    let secrets_path = secrets_dir().map_err(|error| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to get secrets directory: {error}"),
        )
    })?;
    load_or_create_secret(&secrets_path, service_name)
}

/// Async version of get_or_generate_password for async contexts
pub async fn get_or_generate_password(service_name: &str) -> std::io::Result<String> {
    get_or_generate_password_sync(service_name)
}

/// Get the path to the secrets directory
pub fn get_secrets_dir() -> std::io::Result<PathBuf> {
    secrets_dir().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to get secrets directory: {}", e),
        )
    })
}

fn validate_service_name(service_name: &str) -> io::Result<()> {
    let valid = !service_name.is_empty()
        && service_name.len() <= 64
        && service_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Secret service names may contain only letters, numbers, hyphens, and underscores",
        ))
    }
}

fn load_or_create_secret(secrets_path: &Path, service_name: &str) -> io::Result<String> {
    secure_directory(secrets_path)?;
    let secret_file = secrets_path.join(format!("{service_name}.env"));

    match read_secret(&secret_file) {
        Ok(Some(password)) => return Ok(password),
        Ok(None) => {}
        Err(error) => return Err(error),
    }

    let password = generate_random_password(16);
    match create_secret(&secret_file, password.as_bytes()) {
        Ok(()) => {
            eprintln!("💡 Generated new password for {service_name} and saved to {secret_file:?}");
            Ok(password)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => read_secret(&secret_file)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Secret file disappeared")),
        Err(error) => Err(error),
    }
}

fn secure_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Secrets path is not a private directory: {}",
                path.display()
            ),
        ));
    }
    crate::file_system::set_permissions_mode(path, 0o700)
}

fn read_secret(path: &Path) -> io::Result<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Secret path is not a regular file: {}", path.display()),
        ));
    }
    crate::file_system::set_permissions_mode(path, 0o600)?;
    let password = fs::read_to_string(path)?.trim().to_string();
    if password.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Secret file is empty: {}", path.display()),
        ));
    }
    Ok(Some(password))
}

fn create_secret(path: &Path, value: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    if let Err(error) = write_secret(&mut file, value) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    crate::file_system::set_permissions_mode(path, 0o600)
}

fn write_secret(file: &mut File, value: &[u8]) -> io::Result<()> {
    file.write_all(value)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::{load_or_create_secret, validate_service_name};

    #[test]
    fn rejects_secret_path_components() {
        assert!(validate_service_name("postgresql").is_ok());
        assert!(validate_service_name("../postgresql").is_err());
        assert!(validate_service_name("").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn creates_and_repairs_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let secrets = temp.path().join("secrets");
        let first = load_or_create_secret(&secrets, "postgresql").unwrap();
        let secret = secrets.join("postgresql.env");

        assert_eq!(
            std::fs::metadata(&secrets).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&secret).unwrap().permissions().mode() & 0o777,
            0o600
        );

        std::fs::set_permissions(&secrets, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            load_or_create_secret(&secrets, "postgresql").unwrap(),
            first
        );
        assert_eq!(
            std::fs::metadata(&secrets).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&secret).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_secret_files() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let secrets = temp.path().join("secrets");
        std::fs::create_dir(&secrets).unwrap();
        let target = temp.path().join("target");
        std::fs::write(&target, "do-not-read").unwrap();
        symlink(&target, secrets.join("postgresql.env")).unwrap();

        assert!(load_or_create_secret(&secrets, "postgresql").is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "do-not-read");
    }
}
