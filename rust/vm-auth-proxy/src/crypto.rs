//! Cryptographic operations for secret encryption and decryption

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use std::fs;
use std::path::Path;

/// Number of PBKDF2 iterations for key derivation
const PBKDF2_ITERATIONS: u32 = 100_000;

/// Salt length in bytes
const SALT_LENGTH: usize = 32;

/// AES-GCM nonce length in bytes
const NONCE_LENGTH: usize = 12;

const MASTER_KEY_FILE: &str = "master.key";
const MASTER_KEY_BYTES: usize = 32;

/// Encryption key derived from master password and salt
#[derive(Clone)]
pub struct EncryptionKey {
    cipher: Aes256Gcm,
}

impl EncryptionKey {
    /// Derive encryption key from master password and salt
    pub fn derive_from_password(password: &str, salt: &[u8]) -> Result<Self> {
        if salt.len() != SALT_LENGTH {
            return Err(anyhow!("Salt must be {SALT_LENGTH} bytes long"));
        }

        let mut key = [0u8; 32]; // 256 bits for AES-256
        pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);

        let cipher =
            Aes256Gcm::new_from_slice(&key).map_err(|e| anyhow!("Failed to create cipher: {e}"))?;

        Ok(Self { cipher })
    }

    /// Encrypt a plaintext value
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        // Generate random nonce
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        // Encrypt the data
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| anyhow!("Encryption failed: {e}"))?;

        // Combine nonce and ciphertext
        let mut combined = Vec::with_capacity(NONCE_LENGTH + ciphertext.len());
        combined.extend_from_slice(&nonce);
        combined.extend_from_slice(&ciphertext);

        // Encode as base64
        Ok(STANDARD.encode(combined))
    }

    /// Decrypt a ciphertext value
    #[allow(deprecated)]
    pub fn decrypt(&self, encrypted: &str) -> Result<String> {
        // Decode from base64
        let combined = STANDARD
            .decode(encrypted)
            .context("Failed to decode base64")?;

        if combined.len() < NONCE_LENGTH {
            return Err(anyhow!("Encrypted data too short"));
        }

        // Split nonce and ciphertext
        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LENGTH);
        let nonce = Nonce::clone_from_slice(nonce_bytes);

        // Decrypt
        let plaintext = self
            .cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|e| anyhow!("Decryption failed: {e}"))?;

        String::from_utf8(plaintext).context("Decrypted data is not valid UTF-8")
    }
}

/// Generate a cryptographically secure random salt
pub fn generate_salt() -> [u8; SALT_LENGTH] {
    let mut salt = [0u8; SALT_LENGTH];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Generate a secure random authentication token
pub fn generate_auth_token() -> String {
    let mut token_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut token_bytes);
    STANDARD.encode(token_bytes)
}

pub fn get_or_create_master_password(data_dir: &Path) -> Result<String> {
    let key_path = data_dir.join(MASTER_KEY_FILE);
    if key_path.exists() {
        let password = fs::read_to_string(&key_path)
            .with_context(|| format!("Failed to read master key at {}", key_path.display()))?;
        let password = password.trim().to_string();
        if password.is_empty() {
            return Err(anyhow!("Master key at {} is empty", key_path.display()));
        }
        return Ok(password);
    }

    let mut key_bytes = [0u8; MASTER_KEY_BYTES];
    OsRng.fill_bytes(&mut key_bytes);
    let password = STANDARD.encode(key_bytes);
    fs::write(&key_path, format!("{password}\n"))
        .with_context(|| format!("Failed to write master key at {}", key_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to secure master key at {}", key_path.display()))?;
    }

    Ok(password)
}

pub(crate) fn legacy_master_password() -> String {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "vm-user".to_string());

    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "vm-host".to_string());

    format!("vm-auth-proxy-{username}-{hostname}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let password = "test-password";
        let salt = generate_salt();
        let key = EncryptionKey::derive_from_password(password, &salt)
            .expect("should derive key from password");

        let plaintext = "my-secret-api-key";
        let encrypted = key.encrypt(plaintext).expect("should encrypt plaintext");
        let decrypted = key.decrypt(&encrypted).expect("should decrypt ciphertext");

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_different_keys_fail_decryption() {
        let salt = generate_salt();
        let key1 =
            EncryptionKey::derive_from_password("password1", &salt).expect("should derive key 1");
        let key2 =
            EncryptionKey::derive_from_password("password2", &salt).expect("should derive key 2");

        let plaintext = "secret";
        let encrypted = key1.encrypt(plaintext).expect("should encrypt with key 1");

        // Different key should fail to decrypt
        assert!(key2.decrypt(&encrypted).is_err());
    }

    #[test]
    fn test_salt_generation() {
        let salt1 = generate_salt();
        let salt2 = generate_salt();

        // Should generate different salts
        assert_ne!(salt1, salt2);
        assert_eq!(salt1.len(), SALT_LENGTH);
    }

    #[test]
    fn test_auth_token_generation() {
        let token1 = generate_auth_token();
        let token2 = generate_auth_token();

        // Should generate different tokens
        assert_ne!(token1, token2);
        assert!(!token1.is_empty());

        // Should be valid base64
        assert!(STANDARD.decode(&token1).is_ok());
    }

    #[test]
    fn test_master_password_is_persisted_random_value() {
        let temp_dir = tempfile::tempdir().expect("should create temp dir");
        let first = get_or_create_master_password(temp_dir.path())
            .expect("should create persisted master password");
        let second = get_or_create_master_password(temp_dir.path())
            .expect("should read persisted master password");

        assert_eq!(first, second);
        assert_ne!(first, legacy_master_password());
        assert!(temp_dir.path().join(MASTER_KEY_FILE).exists());
    }
}
