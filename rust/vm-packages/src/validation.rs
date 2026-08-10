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
    use super::{validate_label, validate_registry_url, validate_repository_url};

    #[test]
    fn validates_shared_labels_and_urls() {
        assert!(validate_label("package", "@scope/auth").is_ok());
        assert!(validate_label("package", "../auth").is_err());
        assert!(validate_repository_url("ssh://git@example.com/team/auth.git").is_ok());
        assert!(validate_repository_url("https://token@example.com/auth.git").is_err());
        assert!(validate_registry_url("sparse+https://packages.example.com/cargo/").is_ok());
        assert!(validate_registry_url("file:///tmp/packages").is_err());
    }
}
