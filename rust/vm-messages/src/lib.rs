//! vm-messages
//!
//! Reusable message templates for configuration, plugin, and service workflows.
//! General lifecycle copy stays with its command, while shared output behavior
//! lives in `vm-core`.

pub mod messages;

// Re-export the main MESSAGES constant for convenient access
pub use messages::MESSAGES;
