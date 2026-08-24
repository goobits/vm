pub const LINUX_NAME: &str = "vibe-tart-linux-base";
pub const MACOS_NAME: &str = "vibe-tart-sequoia-base";
pub const LINUX_REGISTRY: &str = "ghcr.io/goobits/vm-tart-linux";

pub fn guest_os(name: &str) -> Option<&'static str> {
    match name {
        LINUX_NAME => Some("linux"),
        MACOS_NAME => Some("macos"),
        _ if name
            .strip_prefix(LINUX_NAME)
            .is_some_and(|suffix| suffix.starts_with("-v")) =>
        {
            Some("linux")
        }
        _ => None,
    }
}

pub fn versioned_image() -> String {
    format!("{LINUX_REGISTRY}:v{}", env!("CARGO_PKG_VERSION"))
}

pub fn versioned_cache_name() -> String {
    format!("{LINUX_NAME}-v{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_has_one_versioned_owner() {
        assert_eq!(guest_os(LINUX_NAME), Some("linux"));
        assert_eq!(guest_os(MACOS_NAME), Some("macos"));
        assert_eq!(guest_os(&versioned_cache_name()), Some("linux"));
        assert_eq!(guest_os("custom"), None);
        assert_eq!(
            versioned_image(),
            format!("{LINUX_REGISTRY}:v{}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            versioned_cache_name(),
            format!("{LINUX_NAME}-v{}", env!("CARGO_PKG_VERSION"))
        );
    }
}
