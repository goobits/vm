//! Platform registry for detecting and providing platform implementations.

use crate::traits::PlatformProvider;
use std::sync::Arc;

#[cfg(target_os = "macos")]
use crate::providers::MacOSPlatform;

#[cfg(windows)]
use crate::providers::WindowsPlatform;

#[cfg(unix)]
use crate::providers::UnixPlatform;

/// Platform registry for detecting the current platform and creating providers.
pub struct PlatformRegistry;

impl PlatformRegistry {
    /// Get the platform provider for the current operating system.
    ///
    /// This function automatically detects the current platform and returns
    /// the appropriate provider implementation.
    pub fn current() -> Arc<dyn PlatformProvider> {
        #[cfg(target_os = "macos")]
        return Arc::new(MacOSPlatform);

        #[cfg(windows)]
        return Arc::new(WindowsPlatform);

        #[cfg(all(unix, not(target_os = "macos")))]
        return Arc::new(UnixPlatform);

        #[cfg(not(any(unix, windows)))]
        compile_error!("Unsupported platform - only Unix-like and Windows platforms are supported");
    }

    /// Get a platform provider by name.
    ///
    /// This is useful for testing or when you need to work with a specific
    /// platform provider regardless of the current OS.
    ///
    /// # Arguments
    /// * `name` - Platform name ("unix", "windows", "macos", "linux", "darwin")
    ///
    /// # Returns
    /// Some(provider) if the platform is supported, None otherwise
    pub fn for_name(name: &str) -> Option<Arc<dyn PlatformProvider>> {
        match name.to_lowercase().as_str() {
            "unix" | "linux" => {
                #[cfg(unix)]
                return Some(Arc::new(UnixPlatform));
                #[cfg(not(unix))]
                return None;
            }
            "windows" | "win32" => {
                #[cfg(windows)]
                return Some(Arc::new(WindowsPlatform));
                #[cfg(not(windows))]
                return None;
            }
            "macos" | "darwin" | "osx" => {
                #[cfg(target_os = "macos")]
                return Some(Arc::new(MacOSPlatform));
                #[cfg(not(target_os = "macos"))]
                return None;
            }
            _ => None,
        }
    }

    /// Get the current platform name as a string.
    ///
    /// Returns the canonical platform name for the current OS.
    pub fn current_platform_name() -> &'static str {
        Self::current().name()
    }

    /// Check if a platform is supported by name.
    ///
    /// # Arguments
    /// * `name` - Platform name to check
    ///
    /// # Returns
    /// true if the platform is supported, false otherwise
    pub fn is_platform_supported(name: &str) -> bool {
        Self::for_name(name).is_some()
    }

    /// List all supported platform names.
    ///
    /// Returns a vector of all platform names that can be used with `for_name()`.
    pub fn supported_platforms() -> Vec<&'static str> {
        let mut platforms = Vec::new();

        #[cfg(unix)]
        {
            platforms.extend_from_slice(&["unix", "linux"]);
        }

        #[cfg(windows)]
        {
            platforms.extend_from_slice(&["windows", "win32"]);
        }

        #[cfg(target_os = "macos")]
        {
            platforms.extend_from_slice(&["macos", "darwin", "osx"]);
        }

        platforms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_platform() {
        let platform = PlatformRegistry::current();
        assert!(!platform.name().is_empty());
    }

    #[test]
    fn test_current_platform_name() {
        let name = PlatformRegistry::current_platform_name();
        assert!(!name.is_empty());
        assert!(["unix", "windows", "macos"].contains(&name));
    }

    #[test]
    fn test_supported_platforms() {
        let platforms = PlatformRegistry::supported_platforms();
        assert!(!platforms.is_empty());

        // Current platform should be in the supported list
        let current_name = PlatformRegistry::current_platform_name();
        assert!(platforms.contains(&current_name));
    }

    #[test]
    fn test_platform_support_check() {
        // Current platform should be supported
        let current_name = PlatformRegistry::current_platform_name();
        assert!(PlatformRegistry::is_platform_supported(current_name));

        // Invalid platform should not be supported
        assert!(!PlatformRegistry::is_platform_supported("invalid"));
    }

    #[test]
    fn test_for_name_current_platform() {
        let current_name = PlatformRegistry::current_platform_name();
        let provider = PlatformRegistry::for_name(current_name);
        assert!(provider.is_some());

        if let Some(provider) = provider {
            assert_eq!(provider.name(), current_name);
        }
    }

    #[test]
    fn test_platform_basic_operations() {
        let platform = PlatformRegistry::current();

        // Test basic path operations
        assert!(platform.home_dir().is_ok());
        assert!(platform.user_config_dir().is_ok());
        assert!(platform.user_bin_dir().is_ok());

        // Test executable naming
        let exe_name = platform.executable_name("test");
        assert!(!exe_name.is_empty());

        // Test path operations
        assert!(platform.path_separator() == ':' || platform.path_separator() == ';');
    }
}