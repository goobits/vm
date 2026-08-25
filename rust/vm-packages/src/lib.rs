//! Shared package-infrastructure types and protocol client.
//!
//! This crate intentionally contains no installers, Git operations, build logic,
//! or host filesystem integration. Those operations belong to the managed
//! package-infrastructure services.

mod appliance;
mod build;
mod catalog;
mod client;
mod consumer;
mod credentials;
mod digest;
mod ecosystem;
mod environment;
mod release;
mod resolver;
mod submission;
mod tool_activation;
mod tools;
mod validation;
mod workflow;

pub use appliance::{
    ApplianceConfig, APPLIANCE_DEFINITION_REVISION, COMPOSE_PROJECT, COMPOSE_YAML, GATEWAY_CONFIG,
};
pub use build::{
    CompleteToolBuildRequest, ToolBuildArtifact, ToolBuildFailureKind, ToolBuildRecord,
};
pub use catalog::{PackageDefinition, RegisterPackage};
pub use client::{InfrastructureStatus, PackageInfrastructureClient, PackageInventory};
pub use consumer::{
    ConsumerRecord, ConsumerUsage, CreateRollout, PackageDrift, RegisterConsumer, RolloutRecord,
    RolloutState, RolloutTransition, RolloutValidationRequest,
};
pub use credentials::{
    authorization_token, issue_agent_capability, issue_agent_capability_v2,
    verify_agent_capability, AgentCapabilityClaims, ToolSourceAttestation, AGENT_CAPABILITY_HEADER,
};
pub use digest::{encode_hex, sha256_hex, sha256_reader};
pub use ecosystem::{PackageEcosystem, ParsePackageEcosystemError};
pub use environment::{ClientEnvironment, ManagedClientSettings, RegistryEndpoints};
pub use release::{
    BeginReleaseRequest, CompleteReleaseRequest, PublicationRecord, PublicationRequest,
    PublicationTarget, ReleaseRecord, ReleaseReworkRequest,
};
pub use resolver::{
    InternalPackageCatalog, OverrideAvailability, PackageIdentity, PackageResolver,
    ResolutionAvailability, ResolutionError, ResolutionSource,
};
pub use submission::{
    CheckOutcome, IntegrationRecord, IntegrationRequest, IntegrationReview, PublicApiDiff,
    ReviewDecision, ReviewRequest, SubmissionRecord, ValidationRequest, ValidationResult,
    VersionRecommendation,
};
pub use tool_activation::{
    ClaimToolActivationRequest, FinishToolActivationRequest, PlanToolActivationRequest,
    ToolActivationLease, ToolActivationRecord, ToolActivationState, ToolActivationTarget,
    ToolActivationTargetPlan, ToolActivationTargetState, UpdateToolActivationTargetRequest,
};
pub use tools::{
    artifact_key as tool_artifact_key, tool_artifact_path, validate_sha256, validate_tool_name,
    validate_tool_target, validate_version as validate_tool_version, PublishToolArtifact,
    RegisterTool, ToolArtifactRecord, ToolBuild, ToolBuildSource, ToolDefinition, ToolIndex,
    ToolInventory, ToolKind, ToolPublicationReceipt, ToolSourceManifest, TOOL_SOURCE_SCHEMA,
};
pub use validation::{
    normalize_remote_repository_url, repository_urls_equivalent, validate_label,
    validate_managed_id, validate_registry_url, validate_repository_url, PackageValidationError,
    AUTHENTICATED_GIT_CONFIG,
};
pub use workflow::{
    CheckoutLease, CheckoutRecord, CleanupRequest, CreateCheckout, LeaseRecord, LeaseRequest,
    PackageCheckoutContext, ReceiptKind, SourceKind, TransitionRequest, WorkflowReceipt,
    WorkflowState, WorkflowTransition,
};
