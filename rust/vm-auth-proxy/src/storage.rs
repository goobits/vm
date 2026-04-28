//! Persistent storage for encrypted secrets

use crate::crypto::{
    generate_auth_token, generate_salt, get_or_create_master_password, legacy_master_password,
    EncryptionKey,
};
use crate::types::{Secret, SecretScope, SecretStorage};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// File name for the secrets storage
const SECRETS_FILE: &str = "secrets.json";

/// Directory permissions for auth data
const DIR_PERMISSIONS: u32 = 0o700;

/// File permissions for secrets file
const FILE_PERMISSIONS: u32 = 0o600;

const STORAGE_VERSION_RANDOM_MASTER_KEY: u32 = 2;

/// Secret storage manager
pub struct SecretStore {
    #[allow(dead_code)]
    data_dir: PathBuf,
    storage_file: PathBuf,
    encryption_key: EncryptionKey,
    storage: SecretStorage,
}

impl SecretStore {
    /// Create or load secret store from data directory
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        // Ensure data directory exists with proper permissions
        if !data_dir.exists() {
            fs::create_dir_all(&data_dir).context("Failed to create auth data directory")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = fs::Permissions::from_mode(DIR_PERMISSIONS);
                fs::set_permissions(&data_dir, perms)
                    .context("Failed to set directory permissions")?;
            }
        }

        let storage_file = data_dir.join(SECRETS_FILE);

        // Load or create storage
        let mut storage = if storage_file.exists() {
            Self::load_storage(&storage_file)?
        } else {
            Self::create_new_storage()?
        };

        // Get master password and create encryption key
        let master_password = get_or_create_master_password(&data_dir)?;
        let salt_bytes = STANDARD
            .decode(&storage.salt)
            .context("Failed to decode salt")?;
        let encryption_key = EncryptionKey::derive_from_password(&master_password, &salt_bytes)?;

        if storage.version < STORAGE_VERSION_RANDOM_MASTER_KEY {
            Self::migrate_from_legacy_master_password(&mut storage, &salt_bytes, &encryption_key)?;
        }

        // Generate auth token if not present
        if storage.auth_token.is_none() {
            storage.auth_token = Some(generate_auth_token());
        }

        let store = Self {
            data_dir,
            storage_file,
            encryption_key,
            storage,
        };

        // Save if we made changes (like adding auth token)
        store.save()?;

        Ok(store)
    }

    /// Add or update a secret
    pub fn add_secret(
        &mut self,
        name: &str,
        value: &str,
        scope: SecretScope,
        description: Option<String>,
    ) -> Result<()> {
        // Encrypt the value
        let encrypted_value = self
            .encryption_key
            .encrypt(value)
            .context("Failed to encrypt secret")?;

        // Create or update secret
        let secret = Secret::new(encrypted_value, scope, description);
        self.storage.secrets.insert(name.to_string(), secret);

        // Save to disk
        self.save().context("Failed to save secrets")
    }

    /// Get a secret value (decrypted)
    pub fn get_secret(&self, name: &str) -> Result<Option<String>> {
        if let Some(secret) = self.storage.secrets.get(name) {
            let decrypted = self
                .encryption_key
                .decrypt(&secret.encrypted_value)
                .context("Failed to decrypt secret")?;
            Ok(Some(decrypted))
        } else {
            Ok(None)
        }
    }

    /// Get secret metadata without decrypting
    pub fn get_secret_metadata(&self, name: &str) -> Option<&Secret> {
        self.storage.secrets.get(name)
    }

    /// List all secret names and metadata
    pub fn list_secrets(&self) -> &HashMap<String, Secret> {
        &self.storage.secrets
    }

    /// Remove a secret
    pub fn remove_secret(&mut self, name: &str) -> Result<bool> {
        let removed = self.storage.secrets.remove(name).is_some();
        if removed {
            self.save()
                .context("Failed to save secrets after removal")?;
        }
        Ok(removed)
    }

    /// Get environment variables for a VM
    pub fn get_env_vars_for_vm(
        &self,
        vm_name: &str,
        project_name: Option<&str>,
    ) -> Result<HashMap<String, String>> {
        let mut env_vars = HashMap::new();

        for (name, secret) in &self.storage.secrets {
            let should_include = match &secret.scope {
                SecretScope::Global => true,
                SecretScope::Project(project) => project_name.is_some_and(|p| p == project),
                SecretScope::Instance(instance) => instance == vm_name,
            };

            if should_include {
                let value = self
                    .encryption_key
                    .decrypt(&secret.encrypted_value)
                    .context("Failed to decrypt secret for environment")?;
                env_vars.insert(name.to_uppercase(), value);
            }
        }

        Ok(env_vars)
    }

    /// Get the authentication token
    pub fn get_auth_token(&self) -> Option<&str> {
        self.storage.auth_token.as_deref()
    }

    /// Get the number of stored secrets
    pub fn secret_count(&self) -> usize {
        self.storage.secrets.len()
    }

    /// Create new storage with generated salt
    fn create_new_storage() -> Result<SecretStorage> {
        let salt = generate_salt();
        let salt_b64 = STANDARD.encode(salt);

        Ok(SecretStorage {
            version: STORAGE_VERSION_RANDOM_MASTER_KEY,
            salt: salt_b64,
            secrets: HashMap::new(),
            auth_token: None,
        })
    }

    /// Load storage from file
    fn load_storage(path: &Path) -> Result<SecretStorage> {
        let content = fs::read_to_string(path).context("Failed to read secrets file")?;
        serde_json::from_str(&content).context("Failed to parse secrets file")
    }

    fn migrate_from_legacy_master_password(
        storage: &mut SecretStorage,
        salt_bytes: &[u8],
        new_key: &EncryptionKey,
    ) -> Result<()> {
        let legacy_key = EncryptionKey::derive_from_password(&legacy_master_password(), salt_bytes)
            .context("Failed to derive legacy auth-proxy encryption key")?;

        for (name, secret) in &mut storage.secrets {
            let plaintext = legacy_key
                .decrypt(&secret.encrypted_value)
                .with_context(|| format!("Failed to decrypt legacy secret '{name}'"))?;
            let encrypted_value = new_key
                .encrypt(&plaintext)
                .with_context(|| format!("Failed to re-encrypt migrated secret '{name}'"))?;
            secret.update(encrypted_value);
        }

        storage.version = STORAGE_VERSION_RANDOM_MASTER_KEY;
        Ok(())
    }

    /// Save storage to file
    fn save(&self) -> Result<()> {
        let content =
            serde_json::to_string_pretty(&self.storage).context("Failed to serialize secrets")?;

        fs::write(&self.storage_file, content).context("Failed to write secrets file")?;

        // Set restrictive permissions on the file
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(FILE_PERMISSIONS);
            fs::set_permissions(&self.storage_file, perms)
                .context("Failed to set file permissions")?;
        }

        Ok(())
    }
}

/// Get the default auth data directory
pub fn get_auth_data_dir() -> Result<PathBuf> {
    let home_dir = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home_dir.join(".vm").join("auth"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Secret;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_secret_store_creation() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let store =
            SecretStore::new(temp_dir.path().to_path_buf()).expect("should create secret store");

        assert!(temp_dir.path().join(SECRETS_FILE).exists());
        assert!(store.get_auth_token().is_some());
        assert_eq!(store.secret_count(), 0);
    }

    #[test]
    fn test_add_and_get_secret() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let mut store =
            SecretStore::new(temp_dir.path().to_path_buf()).expect("should create secret store");

        // Add a secret
        store
            .add_secret(
                "test_key",
                "secret_value",
                SecretScope::Global,
                Some("Test secret".to_string()),
            )
            .expect("should add secret");

        // Retrieve it
        let value = store
            .get_secret("test_key")
            .expect("should get secret")
            .expect("secret should have a value");
        assert_eq!(value, "secret_value");

        // Check metadata
        let metadata = store
            .get_secret_metadata("test_key")
            .expect("should get secret metadata");
        assert_eq!(metadata.scope, SecretScope::Global);
        assert_eq!(metadata.description, Some("Test secret".to_string()));
    }

    #[test]
    fn test_legacy_master_password_store_is_migrated() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let salt = generate_salt();
        let legacy_key =
            EncryptionKey::derive_from_password(&legacy_master_password(), &salt).unwrap();
        let encrypted_value = legacy_key.encrypt("legacy-secret").unwrap();

        let mut secrets = HashMap::new();
        secrets.insert(
            "legacy_key".to_string(),
            Secret::new(encrypted_value, SecretScope::Global, None),
        );
        let legacy_storage = SecretStorage {
            version: 1,
            salt: STANDARD.encode(salt),
            secrets,
            auth_token: Some("token".to_string()),
        };
        fs::write(
            temp_dir.path().join(SECRETS_FILE),
            serde_json::to_string_pretty(&legacy_storage).unwrap(),
        )
        .unwrap();

        let store =
            SecretStore::new(temp_dir.path().to_path_buf()).expect("should migrate legacy store");

        assert_eq!(
            store.get_secret("legacy_key").unwrap().as_deref(),
            Some("legacy-secret")
        );

        let migrated = SecretStore::load_storage(&temp_dir.path().join(SECRETS_FILE)).unwrap();
        assert_eq!(migrated.version, STORAGE_VERSION_RANDOM_MASTER_KEY);
    }

    #[test]
    fn test_remove_secret() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let mut store =
            SecretStore::new(temp_dir.path().to_path_buf()).expect("should create secret store");

        // Add and remove a secret
        store
            .add_secret("test_key", "secret_value", SecretScope::Global, None)
            .expect("should add secret");
        assert_eq!(store.secret_count(), 1);

        let removed = store
            .remove_secret("test_key")
            .expect("should remove secret");
        assert!(removed);
        assert_eq!(store.secret_count(), 0);

        // Try to remove non-existent secret
        let removed = store
            .remove_secret("nonexistent")
            .expect("should handle non-existent secret");
        assert!(!removed);
    }

    #[test]
    fn test_env_vars_for_vm() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let mut store =
            SecretStore::new(temp_dir.path().to_path_buf()).expect("should create secret store");

        // Add secrets with different scopes
        store
            .add_secret("global_key", "global_value", SecretScope::Global, None)
            .expect("should add global secret");
        store
            .add_secret(
                "project_key",
                "project_value",
                SecretScope::Project("myproject".to_string()),
                None,
            )
            .expect("should add project secret");
        store
            .add_secret(
                "instance_key",
                "instance_value",
                SecretScope::Instance("myvm".to_string()),
                None,
            )
            .expect("should add instance secret");

        // Get env vars for specific VM
        let env_vars = store
            .get_env_vars_for_vm("myvm", Some("myproject"))
            .expect("should get env vars");

        assert_eq!(
            env_vars.get("GLOBAL_KEY"),
            Some(&"global_value".to_string())
        );
        assert_eq!(
            env_vars.get("PROJECT_KEY"),
            Some(&"project_value".to_string())
        );
        assert_eq!(
            env_vars.get("INSTANCE_KEY"),
            Some(&"instance_value".to_string())
        );

        // Get env vars for different VM
        let env_vars = store
            .get_env_vars_for_vm("othervvm", Some("myproject"))
            .expect("should get env vars for other vm");

        assert_eq!(
            env_vars.get("GLOBAL_KEY"),
            Some(&"global_value".to_string())
        );
        assert_eq!(
            env_vars.get("PROJECT_KEY"),
            Some(&"project_value".to_string())
        );
        assert_eq!(env_vars.get("INSTANCE_KEY"), None);
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().expect("should create temp dir");
        let data_dir = temp_dir.path().to_path_buf();

        // Create store and add secret
        {
            let mut store =
                SecretStore::new(data_dir.clone()).expect("should create initial store");
            store
                .add_secret(
                    "persistent_key",
                    "persistent_value",
                    SecretScope::Global,
                    None,
                )
                .expect("should add persistent secret");
        }

        // Load store again and verify secret persists
        {
            let store = SecretStore::new(data_dir).expect("should load store");
            let value = store
                .get_secret("persistent_key")
                .expect("should get persistent secret")
                .expect("secret should have a value");
            assert_eq!(value, "persistent_value");
        }
    }
}
