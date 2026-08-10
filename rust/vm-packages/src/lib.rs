//! Shared package-infrastructure types and protocol client.
//!
//! This crate intentionally contains no installers, Git operations, build logic,
//! or host filesystem integration. Those operations belong to the managed
//! package-infrastructure services.

mod client;
mod ecosystem;
mod environment;

pub use client::{InfrastructureStatus, PackageInfrastructureClient, PackageInventory};
pub use ecosystem::{PackageEcosystem, ParsePackageEcosystemError};
pub use environment::{ClientEnvironment, RegistryEndpoints};
