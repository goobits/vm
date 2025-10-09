//! VM operation command handlers
//!
//! This module provides command handlers for all VM operations including:
//! - Creation and destruction
//! - Lifecycle management (start, stop, restart)
//! - Interaction (SSH, exec, logs)
//! - Status and listing

// Module declarations
mod create;
mod destroy;
mod helpers;
mod interaction;
mod lifecycle;
mod list;
mod status;

// Re-export all public handlers for external use
pub use create::handle_create;
pub use helpers::handle_get_sync_directory;
pub use interaction::{handle_exec, handle_logs, handle_ssh};
pub use lifecycle::{handle_provision, handle_restart, handle_start, handle_stop};
pub use status::handle_status;

// Legacy exports for backward compatibility
#[allow(unused_imports)]
pub use destroy::{handle_destroy, handle_destroy_enhanced};
#[allow(unused_imports)]
pub use list::{handle_list, handle_list_enhanced};
