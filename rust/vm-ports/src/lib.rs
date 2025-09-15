//! Network port management library.
//!
//! This library provides utilities for managing network port ranges and registries,
//! enabling conflict detection and port allocation for VM projects.

pub mod range;
pub mod registry;

pub use range::PortRange;
pub use registry::{PortRegistry, ProjectEntry};
