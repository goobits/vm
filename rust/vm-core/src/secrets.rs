//! Secrets management for VM services
//!
//! This module provides functionality for generating and retrieving
//! passwords for VM services in a secure manner.

use rand::prelude::*;
use std::path::PathBuf;

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
    let secrets_path = secrets_dir().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to get secrets directory: {}", e),
        )
    })?;
    std::fs::create_dir_all(&secrets_path)?;
    let secret_file = secrets_path.join(format!("{}.env", service_name));

    if secret_file.exists() {
        let password = std::fs::read_to_string(secret_file)?;
        Ok(password.trim().to_string())
    } else {
        let password = generate_random_password(16);
        std::fs::write(&secret_file, &password)?;
        eprintln!(
            "💡 Generated new password for {} and saved to {:?}",
            service_name, secret_file
        );
        Ok(password)
    }
}

/// Async version of get_or_generate_password for async contexts
pub async fn get_or_generate_password(service_name: &str) -> std::io::Result<String> {
    let secrets_path = secrets_dir().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to get secrets directory: {}", e),
        )
    })?;
    tokio::fs::create_dir_all(&secrets_path).await?;
    let secret_file = secrets_path.join(format!("{}.env", service_name));

    if secret_file.exists() {
        let password = tokio::fs::read_to_string(secret_file).await?;
        Ok(password.trim().to_string())
    } else {
        let password = generate_random_password(16);
        tokio::fs::write(&secret_file, &password).await?;
        eprintln!(
            "💡 Generated new password for {} and saved to {:?}",
            service_name, secret_file
        );
        Ok(password)
    }
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
