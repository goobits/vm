//! vm-messages
//!
//! Centralized message templates for the vm CLI.
//! This crate contains only message constants and templates,
//! with no dependencies on other workspace crates.

pub mod categories;
pub mod messages;

// Re-export the main MESSAGES constant for convenient access
pub use messages::MESSAGES;
