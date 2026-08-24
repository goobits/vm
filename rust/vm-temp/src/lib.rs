//! Temporary VM management library.
//!
//! This library provides functionality for managing temporary VMs, including
//! state persistence and mount operations for temporary VM workflows.

mod mount_ops;
mod state;
mod status;
mod temp_ops;

pub use state::StateManager;
pub use temp_ops::TempVmOps;
