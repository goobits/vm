//! VM snapshot management library
//!
//! Provides snapshot creation, restoration, export, and import functionality.

mod archive;
mod base_image;
mod create;
mod docker;
mod export;
mod images;
mod import;
mod manager;
mod metadata;
mod restore;
mod volumes;

// Re-export key types
pub use create::handle_create;
pub use export::handle_export;
pub use import::handle_import;
pub use manager::{SnapshotManager, SnapshotScope};
pub use metadata::{ServiceSnapshot, SnapshotMetadata, VolumeSnapshot};
pub use restore::handle_restore;

/// Calculate optimal concurrency limit based on available CPU count
///
/// Returns a value between 2 and 8 to balance:
/// - Performance on multi-core systems (2-8 concurrent operations)
/// - Protection against resource exhaustion (cap at 8)
/// - Minimal performance on single/dual-core systems (minimum 2)
pub fn optimal_concurrency() -> usize {
    vm_platform::platform::available_parallelism().clamp(2, 8)
}
