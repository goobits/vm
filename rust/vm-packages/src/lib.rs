//! Shared package-infrastructure types and protocol client.
//!
//! This crate intentionally contains no installers, Git operations, build logic,
//! or host filesystem integration. Those operations belong to the managed
//! package-infrastructure services.

mod appliance;
mod catalog;
mod client;
mod consumer;
mod credentials;
mod digest;
mod ecosystem;
mod environment;
mod release;
mod submission;
mod tools;
mod validation;
mod workflow;

pub use appliance::{
    ApplianceConfig, ApplianceState, InfrastructureRuntime, COMPOSE_PROJECT, COMPOSE_YAML,
    GATEWAY_CONFIG, TART_INSTANCE_NAME,
};
pub use catalog::{PackageDefinition, RegisterPackage};
pub use client::{InfrastructureStatus, PackageInfrastructureClient, PackageInventory};
pub use consumer::{
    ConsumerRecord, ConsumerUsage, CreateRollout, PackageDrift, RegisterConsumer, RolloutRecord,
    RolloutState, RolloutTransition, RolloutValidationRequest,
};
pub use credentials::authorization_token;
pub use digest::{encode_hex, sha256_hex};
pub use ecosystem::{PackageEcosystem, ParsePackageEcosystemError};
pub use environment::{ClientEnvironment, RegistryEndpoints};
pub use release::{
    BeginReleaseRequest, CompleteReleaseRequest, PublicationRecord, PublicationRequest,
    ReleaseRecord,
};
pub use submission::{
    CheckOutcome, IntegrationRecord, IntegrationRequest, IntegrationReview, PublicApiDiff,
    ReviewDecision, ReviewRequest, SubmissionRecord, ValidationRequest, ValidationResult,
    VersionRecommendation,
};
pub use tools::{
    artifact_key as tool_artifact_key, tool_artifact_path, validate_sha256, validate_tool_name,
    validate_tool_target, validate_version as validate_tool_version, PublishToolArtifact,
    RegisterTool, ToolArtifactRecord, ToolDefinition, ToolIndex, ToolInventory, ToolKind,
    ToolPublicationReceipt,
};
pub use validation::{
    validate_label, validate_managed_id, validate_registry_url, validate_repository_url,
    PackageValidationError,
};
pub use workflow::{
    CheckoutLease, CheckoutRecord, CleanupRequest, CreateCheckout, LeaseRecord, LeaseRequest,
    ReceiptKind, TransitionRequest, WorkflowReceipt, WorkflowState, WorkflowTransition,
};
