//! VM snapshot management library
//!
//! Provides snapshot creation, restoration, export, and import functionality.

pub mod create;
pub mod docker;
pub mod export;
pub mod import;
pub mod manager;
pub mod metadata;
pub mod restore;

// Re-export key types
pub use manager::SnapshotManager;
pub use metadata::{ServiceSnapshot, SnapshotMetadata, VolumeSnapshot};

/// Calculate optimal concurrency limit based on available CPU count
///
/// Returns a value between 2 and 8 to balance:
/// - Performance on multi-core systems (2-8 concurrent operations)
/// - Protection against resource exhaustion (cap at 8)
/// - Minimal performance on single/dual-core systems (minimum 2)
pub fn optimal_concurrency() -> usize {
    num_cpus::get().clamp(2, 8)
}
