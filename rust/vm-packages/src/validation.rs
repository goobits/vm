use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageValidationError(String);

impl PackageValidationError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for PackageValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for PackageValidationError {}

pub fn validate_label(field: &str, value: &str) -> Result<(), PackageValidationError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && !value.contains("..")
        && !value.starts_with('/')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '/' | '@')
        });
    if valid {
        Ok(())
    } else {
        Err(PackageValidationError::new(format!("invalid {field}")))
    }
}

/// Validate an identifier before using it as one component of a managed path.
pub fn validate_managed_id(field: &str, value: &str) -> Result<(), PackageValidationError> {
    let valid = !value.is_empty()
        && value.len() <= 160
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(PackageValidationError::new(format!(
            "invalid managed {field}"
        )))
    }
}

pub fn validate_repository_url(value: &str) -> Result<(), PackageValidationError> {
    let repository = url::Url::parse(value)
        .map_err(|_| PackageValidationError::new("repository must be an absolute URL"))?;
    if !matches!(repository.scheme(), "https" | "ssh" | "file")
        || repository.password().is_some()
        || (repository.scheme() == "https" && !repository.username().is_empty())
        || repository.query().is_some()
        || repository.fragment().is_some()
    {
        return Err(PackageValidationError::new(
            "repository URL must use https, ssh, or an appliance-local file URL without embedded credentials, query, or fragment",
        ));
    }
    Ok(())
}

/// Normalize a Git remote that must be reachable by package infrastructure.
/// SCP-style SSH remotes are accepted, while host-local file remotes are not.
pub fn normalize_remote_repository_url(value: &str) -> Result<String, PackageValidationError> {
    let candidate = if let Some((authority, path)) =
        value.split_once(':').filter(|(authority, path)| {
            !authority.is_empty()
                && !path.is_empty()
                && !authority.contains('/')
                && (authority.contains('@') || authority.contains('.') || *authority == "localhost")
        }) {
        format!("ssh://{authority}/{}", path.trim_start_matches('/'))
    } else if url::Url::parse(value).is_ok() {
        value.to_string()
    } else {
        return Err(PackageValidationError::new(
            "Git origin must be an absolute HTTPS or SSH repository URL",
        ));
    };
    let parsed = url::Url::parse(&candidate)
        .map_err(|_| PackageValidationError::new("Git origin is not a valid repository URL"))?;
    if parsed.scheme() == "file" {
        return Err(PackageValidationError::new(
            "host-local Git origins are not reachable by package infrastructure",
        ));
    }
    validate_repository_url(&candidate)?;
    Ok(candidate)
}

/// Treat GitHub HTTPS and SSH transports as the same canonical repository.
/// The package appliance stores the original origin for receipts while its
/// credential-isolated Git processes may use either transport.
pub fn repository_urls_equivalent(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    github_repository_identity(left)
        .zip(github_repository_identity(right))
        .is_some_and(|(left, right)| left == right)
}

fn github_repository_identity(value: &str) -> Option<String> {
    let repository = url::Url::parse(value).ok()?;
    if !repository
        .host_str()
        .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
        || !matches!(repository.scheme(), "https" | "ssh")
    {
        return None;
    }
    if repository.scheme() == "ssh" && !matches!(repository.username(), "" | "git") {
        return None;
    }
    let path = repository
        .path()
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or_else(|| repository.path().trim_matches('/'));
    (!path.is_empty()).then(|| path.to_ascii_lowercase())
}

pub fn validate_registry_url(value: &str) -> Result<(), PackageValidationError> {
    let value = value.strip_prefix("sparse+").unwrap_or(value);
    let registry = url::Url::parse(value)
        .map_err(|_| PackageValidationError::new("registry must be an absolute HTTP(S) URL"))?;
    if !matches!(registry.scheme(), "http" | "https")
        || registry.host_str().is_none()
        || !registry.username().is_empty()
        || registry.password().is_some()
        || registry.query().is_some()
        || registry.fragment().is_some()
    {
        return Err(PackageValidationError::new(
            "registry must be an absolute HTTP(S) URL without credentials, query, or fragment",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_remote_repository_url, repository_urls_equivalent, validate_label,
        validate_managed_id, validate_registry_url, validate_repository_url,
    };

    #[test]
    fn validates_shared_labels_and_urls() {
        assert!(validate_label("package", "@scope/auth").is_ok());
        assert!(validate_label("package", "../auth").is_err());
        assert!(validate_repository_url("ssh://git@example.com/team/auth.git").is_ok());
        assert!(validate_repository_url("https://token@example.com/auth.git").is_err());
        assert!(validate_registry_url("sparse+https://packages.example.com/cargo/").is_ok());
        assert!(validate_registry_url("file:///tmp/packages").is_err());
        assert_eq!(
            normalize_remote_repository_url("git@example.com:team/auth.git").unwrap(),
            "ssh://git@example.com/team/auth.git"
        );
        assert!(normalize_remote_repository_url("file:///tmp/auth.git").is_err());
        assert!(repository_urls_equivalent(
            "https://github.com/goobits/auth.git",
            "ssh://git@github.com/goobits/auth.git"
        ));
        assert!(!repository_urls_equivalent(
            "https://github.com/goobits/auth.git",
            "ssh://git@github.com/goobits/security.git"
        ));
        assert!(!repository_urls_equivalent(
            "https://example.com/goobits/auth.git",
            "ssh://git@example.com/goobits/auth.git"
        ));
    }

    #[test]
    fn managed_ids_are_single_safe_path_components() {
        assert!(validate_managed_id("checkout ID", "pkg-auth-20260811-000001").is_ok());
        for invalid in ["", ".", "../source", "scope/auth", "/workspace", "auth.git"] {
            assert!(validate_managed_id("checkout ID", invalid).is_err());
        }
    }
}
