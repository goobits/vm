//! Platform-specific provider implementations.

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

// Re-export platform providers
#[cfg(unix)]
pub use unix::UnixPlatform;

#[cfg(windows)]
pub use windows::WindowsPlatform;

#[cfg(target_os = "macos")]
pub use macos::MacOSPlatform;

// Shell providers (available on all platforms for testing)
pub mod shells;
pub use shells::*;
