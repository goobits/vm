//! VM operation command handlers
//!
//! This module provides command handlers for all VM operations including:
//! - Creation and destruction
//! - Lifecycle management (start, stop)
//! - Interaction (SSH, exec, logs)
//! - Status and listing

// Module declarations
mod copy;
mod create;
mod data_ownership;
mod destroy;
mod fleet;
mod fleet_exec;
mod fleet_remove;
mod fleet_start;
mod fleet_status;
mod helpers;
mod interaction;
mod lifecycle;
mod list;
mod logs;
pub(super) mod target;
pub(super) mod targets;

// Re-export all public handlers for external use
pub use copy::handle_copy;
pub use create::handle_create;
pub use destroy::{confirm_removal, handle_destroy};
pub use interaction::{handle_exec_with_options, handle_ssh};
pub(in crate::commands) use lifecycle::ensure_running;
pub use lifecycle::{handle_restart, handle_start, handle_stop};
pub use logs::{emit_prelaunch_failure as emit_logs_prelaunch_failure, handle_logs};

pub(in crate::commands) use fleet::{
    configured_provider, filter_project_instances, resolve_fleet_targets, FleetProgress,
};
pub use fleet::{
    handle_fleet_exec, handle_fleet_lifecycle, handle_fleet_status, FleetAction, FleetProject,
};
pub(in crate::commands) use fleet_exec::{run as run_fleet_exec, ExecTarget};
pub use fleet_remove::handle_fleet_remove;
pub use fleet_start::handle_fleet_start;
pub(in crate::commands) use fleet_status::collect as collect_fleet_status;
pub use list::handle_declared_project_list;
pub use list::handle_list_enhanced;
pub use list::{collect_declared_project_instances, collect_list_instances, list_output};
pub(in crate::commands) use targets::{is_running_status, InstanceStateFilter};
