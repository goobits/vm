//! Platform-specific provider implementations.

// Shared implementations
pub mod shared;

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

// Shell providers (available on all platforms for testing). Items are reached
// via `providers::shells::*` paths within the crate; no external consumer uses
// the previous `pub use shells::*` re-export, so we drop the glob to keep the
// crate's public surface intentional.
pub mod shells;
