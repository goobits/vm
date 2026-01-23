//! VM operation command handlers
//!
//! This module provides command handlers for all VM operations including:
//! - Creation and destruction
//! - Lifecycle management (start, stop)
//! - Interaction (SSH, exec, logs)
//! - Status and listing

// Module declarations
mod create;
mod destroy;
mod fleet;
mod helpers;
mod interaction;
mod lifecycle;
mod list;
mod status;
mod targets;
mod wait;

// Re-export all public handlers for external use
pub use create::handle_create;
pub use helpers::handle_get_sync_directory;
pub use interaction::{handle_copy, handle_exec, handle_logs, handle_ssh};
pub use lifecycle::{handle_start, handle_stop};
pub use status::handle_status;
pub use wait::handle_wait;

pub use destroy::handle_destroy_enhanced;
pub use fleet::handle_fleet_command;
pub use list::handle_list_enhanced;
