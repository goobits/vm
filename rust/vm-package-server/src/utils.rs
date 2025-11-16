/// Utilities for parsing package filenames across different ecosystems
/// Extract crate name and version from a .crate filename
/// Example: "goobits-pkg-server-0.1.0.crate" -> (Some("goobits-pkg-server"), Some("0.1.0"))
pub fn extract_cargo_name_and_version(filename: &str) -> Option<(String, String)> {
    if filename.ends_with(".crate") {
        let name_part = filename.trim_end_matches(".crate");
        // Find the last dash that separates name from version
        if let Some(dash_pos) = name_part.rfind('-') {
            let after_dash = &name_part[dash_pos + 1..];
            // Check if what comes after dash looks like a version (starts with digit)
            if after_dash.chars().next().is_some_and(|c| c.is_numeric()) {
                let name = name_part[..dash_pos].to_string();
                let version = after_dash.to_string();
                return Some((name, version));
            }
        }
    }
    None
}

/// Extract crate name from Cargo filename (e.g., "hello-world-1.0.0.crate" -> "hello-world")
#[allow(dead_code)]
pub fn extract_cargo_crate_name(filename: &str) -> Option<String> {
    extract_cargo_name_and_version(filename).map(|(name, _version)| name)
}

/// Extract package name from PyPI filename (handles hyphenated names)
///
/// Algorithm: Split on hyphens, find first component starting with digit (version),
/// everything before that is the package name.
///
/// Examples:
/// - "my-package-1.0.0.whl" → Some("my-package")
/// - "django-rest-framework-3.14.0.tar.gz" → Some("django-rest-framework")
/// - "simple-1.0.0.whl" → Some("simple")
pub fn extract_pypi_package_name(filename: &str) -> Option<String> {
    let base = filename
        .strip_suffix(".whl")
        .or_else(|| filename.strip_suffix(".tar.gz"))?;

    let parts: Vec<&str> = base.split('-').collect();
    if parts.is_empty() {
        return None;
    }

    // Find first component that starts with digit (that's the version)
    for (i, part) in parts.iter().enumerate() {
        if i > 0 && !part.is_empty() && part.chars().next().unwrap().is_ascii_digit() {
            let pkg_name = parts[0..i].join("-");
            return Some(crate::normalize_pypi_name(&pkg_name));
        }
    }

    // No version found, treat entire base as package name
    Some(crate::normalize_pypi_name(base))
}

/// Extract both package name and version from PyPI filename
///
/// Examples:
/// - "my-package-1.0.0.whl" → Some(("my-package", "1.0.0"))
/// - "django-rest-framework-3.14.0.tar.gz" → Some(("django-rest-framework", "3.14.0"))
pub fn extract_pypi_package_name_and_version(filename: &str) -> Option<(String, String)> {
    let base = filename
        .strip_suffix(".whl")
        .or_else(|| filename.strip_suffix(".tar.gz"))?;

    let parts: Vec<&str> = base.split('-').collect();
    if parts.is_empty() {
        return None;
    }

    // Find first component that starts with digit
    for (i, part) in parts.iter().enumerate() {
        if i > 0 && !part.is_empty() && part.chars().next().unwrap().is_ascii_digit() {
            let pkg_name = parts[0..i].join("-");
            let normalized = crate::normalize_pypi_name(&pkg_name);
            return Some((normalized, part.to_string()));
        }
    }

    // No version found
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_cargo_name_and_version() {
        assert_eq!(
            extract_cargo_name_and_version("hello-world-1.0.0.crate"),
            Some(("hello-world".to_string(), "1.0.0".to_string()))
        );
        assert_eq!(
            extract_cargo_name_and_version("serde-1.0.195.crate"),
            Some(("serde".to_string(), "1.0.195".to_string()))
        );
        assert_eq!(
            extract_cargo_name_and_version("my-awesome-crate-0.1.0.crate"),
            Some(("my-awesome-crate".to_string(), "0.1.0".to_string()))
        );
        // Invalid cases
        assert_eq!(extract_cargo_name_and_version("invalid.tar.gz"), None);
        assert_eq!(extract_cargo_name_and_version("no-version.crate"), None);
        assert_eq!(extract_cargo_name_and_version("no-extension"), None);
    }

    #[test]
    fn test_extract_cargo_crate_name() {
        assert_eq!(
            extract_cargo_crate_name("hello-world-1.0.0.crate"),
            Some("hello-world".to_string())
        );
        assert_eq!(
            extract_cargo_crate_name("serde-1.0.195.crate"),
            Some("serde".to_string())
        );
        assert_eq!(
            extract_cargo_crate_name("my-awesome-crate-0.1.0.crate"),
            Some("my-awesome-crate".to_string())
        );
        // Invalid cases
        assert_eq!(extract_cargo_crate_name("invalid.tar.gz"), None);
        assert_eq!(extract_cargo_crate_name("no-version.crate"), None);
        assert_eq!(extract_cargo_crate_name("no-extension"), None);
    }

    #[test]
    fn test_extract_pypi_package_name_simple() {
        assert_eq!(
            extract_pypi_package_name("django-1.0.0.whl"),
            Some("django".to_string())
        );
    }

    #[test]
    fn test_extract_pypi_package_name_hyphenated() {
        assert_eq!(
            extract_pypi_package_name("my-package-1.0.0.whl"),
            Some("my-package".to_string())
        );
        assert_eq!(
            extract_pypi_package_name("django-rest-framework-3.14.0.tar.gz"),
            Some("django-rest-framework".to_string())
        );
    }

    #[test]
    fn test_extract_pypi_package_name_normalization() {
        assert_eq!(
            extract_pypi_package_name("My_Package-1.0.0.whl"),
            Some("my-package".to_string())
        );
        assert_eq!(
            extract_pypi_package_name("my.package-2.0.0.whl"),
            Some("my-package".to_string())
        );
    }

    #[test]
    fn test_extract_pypi_package_name_and_version() {
        assert_eq!(
            extract_pypi_package_name_and_version("my-package-1.0.0.whl"),
            Some(("my-package".to_string(), "1.0.0".to_string()))
        );
        assert_eq!(
            extract_pypi_package_name_and_version("newest-package-2.5.0.tar.gz"),
            Some(("newest-package".to_string(), "2.5.0".to_string()))
        );
    }
}
