//! Cryptographic hashing utilities for package integrity verification

/// Calculate SHA256 hash of data.
///
/// Computes the SHA256 hash of the provided byte data and returns it as a
/// lowercase hexadecimal string. This is commonly used for package integrity
/// verification and checksum generation.
///
/// # Examples
///
/// ```
/// # use vm_package_server::hash_utils::sha256_hash;
/// let data = b"hello world";
/// let hash = sha256_hash(data);
/// assert_eq!(hash.len(), 64); // SHA256 produces 64 hex characters
/// ```
pub fn sha256_hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Calculate SHA1 hash of data (for npm).
///
/// Computes the SHA1 hash of the provided byte data and returns it as a
/// lowercase hexadecimal string. This is specifically used for npm package
/// integrity checks, as npm uses SHA1 hashes in its metadata.
///
/// # Examples
///
/// ```
/// # use vm_package_server::hash_utils::sha1_hash;
/// let data = b"hello world";
/// let hash = sha1_hash(data);
/// assert_eq!(hash.len(), 40); // SHA1 produces 40 hex characters
/// ```
pub fn sha1_hash(data: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hash() {
        let data = b"hello world";
        let hash = sha256_hash(data);
        assert_eq!(hash.len(), 64);
        // Known SHA256 hash for "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_sha1_hash() {
        let data = b"hello world";
        let hash = sha1_hash(data);
        assert_eq!(hash.len(), 40);
        // Known SHA1 hash for "hello world"
        assert_eq!(hash, "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed");
    }
}
