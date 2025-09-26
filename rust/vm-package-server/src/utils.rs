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

/// Extract package name from PyPI filename
#[allow(dead_code)]
pub fn extract_pypi_package_name(filename: &str) -> Option<String> {
    // Handle wheel files (.whl)
    if filename.ends_with(".whl") {
        // Format: package_name-version-python-etc.whl
        if let Some(dash_pos) = filename.find('-') {
            return Some(filename[..dash_pos].replace('_', "-"));
        }
    }

    // Handle source distributions (.tar.gz)
    if filename.ends_with(".tar.gz") {
        let name_part = filename.trim_end_matches(".tar.gz");
        // Format: package_name-version.tar.gz
        if let Some(dash_pos) = name_part.rfind('-') {
            // Check if what comes after dash looks like a version
            let after_dash = &name_part[dash_pos + 1..];
            if after_dash
                .chars()
                .next()
                .is_some_and(|c| c.is_numeric() || c == 'v')
            {
                return Some(name_part[..dash_pos].to_string());
            }
        }
    }

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
    fn test_extract_pypi_package_name() {
        assert_eq!(
            extract_pypi_package_name("requests-2.28.0-py3-none-any.whl"),
            Some("requests".to_string())
        );
        assert_eq!(
            extract_pypi_package_name("numpy-1.24.0.tar.gz"),
            Some("numpy".to_string())
        );
        assert_eq!(
            extract_pypi_package_name("some_package-1.0.0-py3-none-any.whl"),
            Some("some-package".to_string())
        );
    }
}
