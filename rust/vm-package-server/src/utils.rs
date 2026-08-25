//! Filename parsing shared by the Python registry handlers.

use vm_packages::{PackageEcosystem, PackageIdentity};

fn normalized_python_name(name: &str) -> Option<String> {
    PackageIdentity::new(PackageEcosystem::Python, name)
        .ok()
        .map(|package| package.name)
}

/// Extract and normalize a Python package name from a wheel or source archive.
pub fn extract_pypi_package_name(filename: &str) -> Option<String> {
    let base = filename
        .strip_suffix(".whl")
        .or_else(|| filename.strip_suffix(".tar.gz"))?;
    let parts = base.split('-').collect::<Vec<_>>();

    for (index, part) in parts.iter().enumerate().skip(1) {
        if part
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
        {
            return normalized_python_name(&parts[..index].join("-"));
        }
    }

    normalized_python_name(base)
}

#[cfg(test)]
mod tests {
    use super::extract_pypi_package_name;

    #[test]
    fn extracts_simple_name() {
        assert_eq!(
            extract_pypi_package_name("django-1.0.0.whl").as_deref(),
            Some("django")
        );
    }

    #[test]
    fn extracts_hyphenated_name() {
        assert_eq!(
            extract_pypi_package_name("django-rest-framework-3.14.0.tar.gz").as_deref(),
            Some("django-rest-framework")
        );
    }

    #[test]
    fn normalizes_python_name() {
        assert_eq!(
            extract_pypi_package_name("My_Package-1.0.0.whl").as_deref(),
            Some("my-package")
        );
        assert_eq!(
            extract_pypi_package_name("my.package-2.0.0.whl").as_deref(),
            Some("my-package")
        );
    }
}
