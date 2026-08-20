//! Temporary VM management library.
//!
//! This library provides functionality for managing temporary VMs, including
//! state persistence, mount operations, and CLI utilities for temporary VM workflows.

pub mod cli;
pub mod models;
pub mod mount_ops;
pub mod state;
mod status;
pub mod temp_ops;

// Root re-exports define the primary API; mount parsing helpers stay internal.
pub use models::{MountPermission, TempVmState};
pub use state::StateManager;
pub use temp_ops::TempVmOps;
