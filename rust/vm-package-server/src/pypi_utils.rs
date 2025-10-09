//! PyPI-specific utility functions

use regex::Regex;
use std::sync::OnceLock;

/// Normalize PyPI package name according to PEP 503.
///
/// This function normalizes package names by converting them to lowercase and
/// replacing runs of `[-_.]+` with a single `-` character.
///
/// # Examples
///
/// ```
/// # use vm_package_server::pypi_utils::normalize_pypi_name;
/// assert_eq!(normalize_pypi_name("Django-REST-framework"), "django-rest-framework");
/// assert_eq!(normalize_pypi_name("some_package"), "some-package");
/// ```
pub fn normalize_pypi_name(name: &str) -> String {
    static PYPI_NAME_REGEX: OnceLock<Regex> = OnceLock::new();
    let re = PYPI_NAME_REGEX.get_or_init(|| {
        Regex::new(r"[-_.]+").unwrap_or_else(|e| {
            panic!("Failed to compile PyPI name normalization regex: {}", e)
        })
    });
    re.replace_all(&name.to_lowercase(), "-").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_pypi_name() {
        assert_eq!(normalize_pypi_name("Django-REST-framework"), "django-rest-framework");
        assert_eq!(normalize_pypi_name("some_package"), "some-package");
        assert_eq!(normalize_pypi_name("package.name"), "package-name");
    }
}