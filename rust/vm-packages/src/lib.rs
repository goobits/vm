//! Shared package-infrastructure types and protocol client.
//!
//! This crate intentionally contains no installers, Git operations, build logic,
//! or host filesystem integration. Those operations belong to the managed
//! package-infrastructure services.

mod appliance;
mod client;
mod credentials;
mod ecosystem;
mod environment;
mod workflow;

pub use appliance::{
    ApplianceConfig, ApplianceState, InfrastructureRuntime, COMPOSE_PROJECT, COMPOSE_YAML,
    GATEWAY_CONFIG, TART_BASE_NAME, TART_INSTANCE_NAME,
};
pub use client::{InfrastructureStatus, PackageInfrastructureClient, PackageInventory};
pub use credentials::authorization_token;
pub use ecosystem::{PackageEcosystem, ParsePackageEcosystemError};
pub use environment::{ClientEnvironment, RegistryEndpoints};
pub use workflow::{
    CheckoutLease, CheckoutRecord, CreateCheckout, LeaseRecord, LeaseRequest, ReceiptKind,
    TransitionRequest, WorkflowReceipt, WorkflowState, WorkflowTransition,
};
