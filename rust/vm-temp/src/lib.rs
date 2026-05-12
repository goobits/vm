//! Temporary VM management library.
//!
//! This library provides functionality for managing temporary VMs, including
//! state persistence, mount operations, and CLI utilities for temporary VM workflows.

pub mod cli;
pub mod models;
pub mod mount_ops;
pub mod state;
pub mod temp_ops;

// Explicit public surface. Internal items (e.g. `mount_ops::MountParser`,
// `state::StateError`) remain reachable via their module paths but are not
// part of the crate's primary API.
pub use models::{MountPermission, TempVmState};
pub use state::StateManager;
pub use temp_ops::TempVmOps;
